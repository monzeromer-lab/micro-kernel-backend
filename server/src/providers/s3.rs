//! S3 Provider — ureq-backed sync HTTP.

use std::io::Read;
use std::time::Duration;
use super::super::services::ServiceProvider;
use wasm_module::S3Handle;

#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String, pub region: String,
    pub access_key: String, pub secret_key: String,
    pub session_token: Option<String>, pub force_path_style: bool,
    pub timeout: Option<Duration>, pub max_retries: u32,
}
impl Default for S3Config {
    fn default() -> Self {
        Self { endpoint: "http://localhost:9000".into(), region: "us-east-1".into(),
            access_key: "minioadmin".into(), secret_key: "minioadmin".into(),
            session_token: None, force_path_style: true,
            timeout: Some(Duration::from_secs(30)), max_retries: 0 }
    }
}

pub struct S3Provider { agent: ureq::Agent, config: S3Config }

impl S3Provider {
    pub fn connect(config: S3Config) -> Result<Self, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(config.timeout.unwrap_or(Duration::from_secs(30)))
            .build();
        Ok(Self { agent, config })
    }

    fn object_url(&self, b: &str, k: &str) -> String {
        let ep = self.config.endpoint.trim_end_matches('/');
        if self.config.force_path_style { format!("{ep}/{b}/{k}") } else { format!("{ep}/{k}") }
    }
    fn bucket_url(&self, b: &str) -> String {
        let ep = self.config.endpoint.trim_end_matches('/');
        if self.config.force_path_style { format!("{ep}/{b}") } else { ep.to_string() }
    }
    fn req(&self, method: &str, url: &str) -> ureq::Request {
        let mut r = match method {
            "GET" => self.agent.get(url), "PUT" => self.agent.put(url),
            "DELETE" => self.agent.delete(url), _ => self.agent.get(url),
        };
        if !self.config.access_key.is_empty() {
            r = r.set("Authorization", &format!("AWS {}:{}", self.config.access_key, self.config.secret_key));
        }
        r.set("x-amz-content-sha256", "UNSIGNED-PAYLOAD")
    }
}

impl ServiceProvider for S3Provider {
    fn call(&self, _m: &str, payload: &[u8]) -> Vec<u8> {
        let body = String::from_utf8_lossy(payload);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let cmd = v["cmd"].as_str().unwrap_or("GET");
        let b = v["bucket"].as_str().unwrap_or("");
        let k = v["key"].as_str().unwrap_or("");

        let result: Result<String, String> = (|| match cmd {
            "PUT" => {
                let data = v["data"].as_str().unwrap_or("");
                self.req("PUT", &self.object_url(b, k)).send_string(data)
                    .map(|r| r.into_string().unwrap_or_default()).map_err(|e| e.to_string())
            }
            "GET" => self.req("GET", &self.object_url(b, k)).call()
                .map(|r| r.into_string().unwrap_or_default()).map_err(|e| e.to_string()),
            "DELETE" => self.req("DELETE", &self.object_url(b, k)).call()
                .map(|_| "{\"deleted\":true}".to_string()).map_err(|e| e.to_string()),
            "LIST" => {
                let p = v["prefix"].as_str().unwrap_or("");
                self.req("GET", &format!("{}?list-type=2&prefix={p}", self.bucket_url(b))).call()
                    .map(|r| r.into_string().unwrap_or_default()).map_err(|e| e.to_string())
            }
            _ => Err(format!("unknown S3 command: {cmd}")),
        })();
        match result { Ok(j) => j.into_bytes(), Err(e) => serde_json::json!({"error":e}).to_string().into_bytes() }
    }
}

impl S3Handle for S3Provider {
    fn put(&self, b: &str, k: &str, data: &[u8]) -> Result<String, String> {
        self.req("PUT", &self.object_url(b, k)).send_bytes(data)
            .map(|_| k.to_string()).map_err(|e| e.to_string())
    }
    fn get(&self, b: &str, k: &str) -> Result<Vec<u8>, String> {
        let resp = self.req("GET", &self.object_url(b, k)).call().map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        resp.into_reader().read_to_end(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    }
    fn delete(&self, b: &str, k: &str) -> Result<bool, String> {
        self.req("DELETE", &self.object_url(b, k)).call()
            .map(|r| r.status() == 204 || r.status() == 200).map_err(|e| e.to_string())
    }
    fn list(&self, b: &str, prefix: Option<&str>) -> Result<String, String> {
        let mut url = format!("{}?list-type=2", self.bucket_url(b));
        if let Some(p) = prefix { url.push_str(&format!("&prefix={p}")); }
        self.req("GET", &url).call().map(|r| r.into_string().unwrap_or_default()).map_err(|e| e.to_string())
    }
}
