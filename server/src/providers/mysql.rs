//! MySQL Provider — full sqlx-backed implementation.
//!
//! ```rust,ignore
//! let my = MySqlProvider::connect(MySqlConfig {
//!     url: "mysql://user:pass@localhost/db".into(),
//!     max_connections: 10,
//!     ..Default::default()
//! }).await?;
//! ```

use std::time::Duration;
use sqlx::{Column, Row};

use super::super::services::ServiceProvider;
use wasm_module::MySqlHandle;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MySqlConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub max_lifetime: Option<Duration>,
    pub acquire_timeout: Option<Duration>,
    pub statement_timeout: Option<Duration>,
}

impl Default for MySqlConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            max_connections: 10,
            min_connections: 1,
            max_lifetime: Some(Duration::from_secs(1800)),
            acquire_timeout: Some(Duration::from_secs(10)),
            statement_timeout: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct MySqlProvider {
    pool: sqlx::MySqlPool,
}

impl MySqlProvider {
    pub async fn connect(config: MySqlConfig) -> Result<Self, sqlx::Error> {
        if config.url.is_empty() {
            return Err(sqlx::Error::Configuration("empty DATABASE_URL".into()));
        }
        let opts: sqlx::mysql::MySqlConnectOptions = config
            .url
            .parse()
            .map_err(|e| sqlx::Error::Configuration(Box::new(e)))?;

        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .max_lifetime(config.max_lifetime)
            .acquire_timeout(config.acquire_timeout.unwrap_or(Duration::from_secs(10)))
            .connect_with(opts)
            .await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &sqlx::MySqlPool {
        &self.pool
    }

    async fn query_rows_json(&self, sql: &str) -> Result<String, String> {
        let rows = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        serialize_rows(&rows)
    }

    async fn execute_sql(&self, sql: &str) -> Result<u64, String> {
        let result = sqlx::query(sql)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(result.rows_affected())
    }
}

// ---------------------------------------------------------------------------
// ServiceProvider impl
// ---------------------------------------------------------------------------

impl ServiceProvider for MySqlProvider {
    fn call(&self, method: &str, payload: &[u8]) -> Vec<u8> {
        let body = String::from_utf8_lossy(payload).to_string();
        let v: serde_json::Value =
            serde_json::from_str(&body).unwrap_or(serde_json::json!({ "sql": body }));
        let sql = v["sql"].as_str().unwrap_or(&body).to_string();
        let kind = v["kind"].as_str().unwrap_or("query");

        let rt = tokio::runtime::Handle::current();
        let result: Result<String, String> = rt.block_on(async {
            match kind {
                "query" | "query_with" => self.query_rows_json(&sql).await,
                "execute" => {
                    let rows = self.execute_sql(&sql).await?;
                    serde_json::to_string(&serde_json::json!({ "rows_affected": rows }))
                        .map_err(|e| e.to_string())
                }
                _ => Err(format!("unknown query kind: {kind}")),
            }
        });

        match result {
            Ok(json) => json.into_bytes(),
            Err(e) => serde_json::json!({ "error": e }).to_string().into_bytes(),
        }
    }
}

// ---------------------------------------------------------------------------
// MySqlHandle impl
// ---------------------------------------------------------------------------

impl MySqlHandle for MySqlProvider {
    fn query(&self, sql: &str) -> Result<String, String> {
        tokio::runtime::Handle::current().block_on(self.query_rows_json(sql))
    }

    fn execute(&self, sql: &str) -> Result<u64, String> {
        tokio::runtime::Handle::current().block_on(self.execute_sql(sql))
    }

    fn query_with(&self, sql: &str, params: &[&str]) -> Result<String, String> {
        let rt = tokio::runtime::Handle::current();
        let mut query = sqlx::query(sql);
        for p in params {
            query = query.bind(p);
        }
        rt.block_on(async {
            let rows = query
                .fetch_all(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
            serialize_rows(&rows)
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn serialize_rows(
    rows: &[sqlx::mysql::MySqlRow],
) -> Result<String, String> {
    let json_rows: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let mut map = serde_json::Map::new();
            for (i, col) in row.columns().iter().enumerate() {
                let val: String = row
                    .try_get::<String, _>(i)
                    .unwrap_or_else(|_| "<unsupported>".into());
                map.insert(col.name().to_string(), serde_json::Value::String(val));
            }
            serde_json::Value::Object(map)
        })
        .collect();

    serde_json::to_string(&serde_json::json!({ "rows": json_rows, "count": json_rows.len() }))
        .map_err(|e| e.to_string())
}
