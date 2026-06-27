//! HTTP Client Provider — ureq-backed sync implementation.

use std::time::Duration;
use super::super::services::ServiceProvider;
use wasm_module::HttpHandle;

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub user_agent: String, pub timeout: Option<Duration>,
    pub connect_timeout: Option<Duration>, pub max_redirects: Option<usize>,
    pub danger_accept_invalid_certs: bool,
}
impl Default for HttpConfig {
    fn default() -> Self {
        Self { user_agent: "wasm-kernel/0.1".into(), timeout: Some(Duration::from_secs(30)),
            connect_timeout: Some(Duration::from_secs(10)), max_redirects: Some(10),
            danger_accept_invalid_certs: false }
    }
}

pub struct HttpProvider { agent: ureq::Agent }

impl HttpProvider {
    pub fn connect(config: HttpConfig) -> Result<Self, String> {
        let mut builder = ureq::AgentBuilder::new().user_agent(&config.user_agent);
        if let Some(t) = config.timeout { builder = builder.timeout(t); }
        if let Some(t) = config.connect_timeout { builder = builder.timeout_connect(t); }
        Ok(Self { agent: builder.build() })
    }
}

impl ServiceProvider for HttpProvider {
    fn call(&self, method: &str, payload: &[u8]) -> Vec<u8> {
        let body = String::from_utf8_lossy(payload);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::json!({"url":body}));
        let url = v["url"].as_str().unwrap_or(&body);
        let method = v["method"].as_str().unwrap_or(method).to_uppercase();
        let req_body = v["body"].as_str().map(|s| s.to_string());

        let result: Result<String, String> = (|| {
            let resp = match method.as_str() {
                "GET" => self.agent.get(url).call(),
                "POST" => self.agent.post(url).send_string(&req_body.unwrap_or_default()),
                "PUT" => self.agent.put(url).send_string(&req_body.unwrap_or_default()),
                "DELETE" => self.agent.delete(url).call(),
                m => return Err(format!("unsupported method: {m}")),
            }.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({"status":resp.status(),"body":resp.into_string().unwrap_or_default()}).to_string())
        })();
        match result { Ok(j) => j.into_bytes(), Err(e) => serde_json::json!({"error":e}).to_string().into_bytes() }
    }
}

impl HttpHandle for HttpProvider {
    fn get(&self, url: &str) -> Result<String, String> {
        self.agent.get(url).call().map(|r| r.into_string().unwrap_or_default()).map_err(|e| e.to_string())
    }
    fn post(&self, url: &str, body: &str) -> Result<String, String> {
        self.agent.post(url).send_string(body).map(|r| r.into_string().unwrap_or_default()).map_err(|e| e.to_string())
    }
    fn put(&self, url: &str, body: &str) -> Result<String, String> {
        self.agent.put(url).send_string(body).map(|r| r.into_string().unwrap_or_default()).map_err(|e| e.to_string())
    }
    fn delete(&self, url: &str) -> Result<String, String> {
        self.agent.delete(url).call().map(|r| r.into_string().unwrap_or_default()).map_err(|e| e.to_string())
    }
}
