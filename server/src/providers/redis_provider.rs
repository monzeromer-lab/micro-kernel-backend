//! Redis Provider — full redis-rs backed implementation.
//!
//! Uses a single connection behind a Mutex for simplicity.

use std::sync::Mutex;
use std::time::Duration;

use super::super::services::ServiceProvider;
use wasm_module::RedisHandle;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub url: String,
    pub connection_timeout: Option<Duration>,
    pub default_ttl_seconds: Option<u64>,
    pub key_prefix: Option<String>,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".into(),
            connection_timeout: Some(Duration::from_secs(5)),
            default_ttl_seconds: None,
            key_prefix: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct RedisProvider {
    conn: Mutex<redis::Connection>,
    config: RedisConfig,
}

impl RedisProvider {
    pub fn connect(config: RedisConfig) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(config.url.as_str())?;
        let conn = client.get_connection()?;
        Ok(Self {
            conn: Mutex::new(conn),
            config,
        })
    }

    fn prefixed(&self, key: &str) -> String {
        match &self.config.key_prefix {
            Some(prefix) => format!("{prefix}:{key}"),
            None => key.to_string(),
        }
    }

    fn with_conn<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&mut redis::Connection) -> Result<T, redis::RedisError>,
    {
        let mut conn = self.conn.lock().map_err(|e| e.to_string())?;
        f(&mut conn).map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// ServiceProvider impl
// ---------------------------------------------------------------------------

impl ServiceProvider for RedisProvider {
    fn call(&self, _method: &str, payload: &[u8]) -> Vec<u8> {
        let body = String::from_utf8_lossy(payload).to_string();
        let v: serde_json::Value =
            serde_json::from_str(&body).unwrap_or(serde_json::json!({ "cmd": body }));

        let cmd = v["cmd"].as_str().unwrap_or("PING");
        let key = v["key"].as_str().unwrap_or("");
        let key = self.prefixed(key);

        let result: Result<String, String> = self.with_conn(|conn| match cmd {
            "GET" => {
                let val: Option<String> = redis::cmd("GET").arg(&key).query(conn)?;
                Ok(serde_json::json!({ "value": val }).to_string())
            }
            "SET" => {
                let value = v["value"].as_str().unwrap_or("");
                redis::cmd("SET").arg(&key).arg(value).query::<()>(conn)?;
                if let Some(ttl) = v["ttl"].as_u64().or(self.config.default_ttl_seconds) {
                    redis::cmd("EXPIRE").arg(&key).arg(ttl as usize).query::<i32>(conn)?;
                }
                Ok(serde_json::json!({ "status": "ok" }).to_string())
            }
            "DEL" => {
                let keys: Vec<String> = v["keys"]
                    .as_array()
                    .map(|a| a.iter().map(|k| self.prefixed(k.as_str().unwrap_or(""))).collect())
                    .unwrap_or_else(|| vec![key]);
                let refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
                let deleted: u64 = redis::cmd("DEL").arg(&refs[..]).query(conn)?;
                Ok(serde_json::json!({ "deleted": deleted }).to_string())
            }
            "INCRBY" => {
                let amount: i64 = v["amount"].as_i64().unwrap_or(1);
                let val: i64 = redis::cmd("INCRBY").arg(&key).arg(amount).query(conn)?;
                Ok(serde_json::json!({ "value": val }).to_string())
            }
            "EXISTS" => {
                let exists: bool = redis::cmd("EXISTS").arg(&key).query(conn)?;
                Ok(serde_json::json!({ "exists": exists }).to_string())
            }
            _ => return Err(redis::RedisError::from((
                redis::ErrorKind::ClientError, "unknown command"
            ))),
        });

        match result {
            Ok(json) => json.into_bytes(),
            Err(e) => serde_json::json!({ "error": e }).to_string().into_bytes(),
        }
    }
}

// ---------------------------------------------------------------------------
// RedisHandle impl
// ---------------------------------------------------------------------------

impl RedisHandle for RedisProvider {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        let key = self.prefixed(key);
        self.with_conn(|conn| {
            let val: Option<String> = redis::cmd("GET").arg(&key).query(conn)?;
            Ok(val)
        })
    }

    fn set(&self, key: &str, value: &str, ttl_seconds: Option<u64>) -> Result<(), String> {
        let key = self.prefixed(key);
        self.with_conn(|conn| {
            redis::cmd("SET").arg(&key).arg(value).query::<()>(conn)?;
            if let Some(ttl) = ttl_seconds.or(self.config.default_ttl_seconds) {
                redis::cmd("EXPIRE").arg(&key).arg(ttl as usize).query::<i32>(conn)?;
            }
            Ok(())
        })
    }

    fn del(&self, keys: &[&str]) -> Result<u64, String> {
        if keys.is_empty() {
            return Ok(0);
        }
        let prefixed: Vec<String> = keys.iter().map(|k| self.prefixed(k)).collect();
        let refs: Vec<&str> = prefixed.iter().map(|s| s.as_str()).collect();
        self.with_conn(|conn| {
            let deleted: u64 = redis::cmd("DEL").arg(&refs[..]).query(conn)?;
            Ok(deleted)
        })
    }

    fn incr(&self, key: &str, amount: Option<i64>) -> Result<i64, String> {
        let key = self.prefixed(key);
        let amt = amount.unwrap_or(1);
        self.with_conn(|conn| {
            let val: i64 = redis::cmd("INCRBY").arg(&key).arg(amt).query(conn)?;
            Ok(val)
        })
    }

    fn exists(&self, key: &str) -> Result<bool, String> {
        let key = self.prefixed(key);
        self.with_conn(|conn| {
            let exists: bool = redis::cmd("EXISTS").arg(&key).query(conn)?;
            Ok(exists)
        })
    }
}
