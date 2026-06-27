//! Postgres Provider — full sqlx-backed implementation.
//!
//! ```rust,ignore
//! let pg = PostgresProvider::connect(PostgresConfig {
//!     url: "postgres://user:pass@localhost/db".into(),
//!     max_connections: 10,
//!     ..Default::default()
//! }).await?;
//! ```

use std::sync::Arc;
use std::time::Duration;
use sqlx::{Column, Row};

use super::super::services::ServiceProvider;
use wasm_module::PostgresHandle;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Full configuration for a Postgres connection pool.
#[derive(Debug, Clone)]
pub struct PostgresConfig {
    /// Connection URL: `postgres://user:password@host:port/database`
    pub url: String,
    /// Maximum active connections in the pool.
    pub max_connections: u32,
    /// Minimum idle connections to keep ready.
    pub min_connections: u32,
    /// Maximum lifetime of a connection before it is recycled.
    pub max_lifetime: Option<Duration>,
    /// How long to wait for a connection from the pool before timing out.
    pub acquire_timeout: Option<Duration>,
    /// How long a single query can run before being cancelled.
    pub statement_timeout: Option<Duration>,
    /// Application name sent to Postgres (visible in `pg_stat_activity`).
    pub application_name: Option<String>,
    /// SSL mode: "disable", "prefer", "require", "verify-ca", "verify-full".
    pub ssl_mode: Option<String>,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            max_connections: 10,
            min_connections: 1,
            max_lifetime: Some(Duration::from_secs(1800)),
            acquire_timeout: Some(Duration::from_secs(10)),
            statement_timeout: None,
            application_name: None,
            ssl_mode: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// A real Postgres provider backed by sqlx.
///
/// Implements both [`ServiceProvider`] (raw access) and [`PostgresHandle`]
/// (typed, ergonomic API exposed to WASM modules).
pub struct PostgresProvider {
    pool: sqlx::PgPool,
}

impl PostgresProvider {
    /// Create and connect a new Postgres provider.
    pub async fn connect(config: PostgresConfig) -> Result<Self, sqlx::Error> {
        if config.url.is_empty() {
            return Err(sqlx::Error::Configuration("empty DATABASE_URL".into()));
        }
        let mut opts: sqlx::postgres::PgConnectOptions = config
            .url
            .parse()
            .map_err(|e| sqlx::Error::Configuration(Box::new(e)))?;

        if let Some(app) = &config.application_name {
            opts = opts.application_name(app);
        }
        if let Some(ssl) = &config.ssl_mode {
            opts = opts.ssl_mode(
                ssl.parse()
                    .map_err(|_| sqlx::Error::Configuration("invalid ssl_mode".into()))?,
            );
        }

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .max_lifetime(config.max_lifetime)
            .acquire_timeout(config.acquire_timeout.unwrap_or(Duration::from_secs(10)))
            .connect_with(opts)
            .await?;

        Ok(Self { pool })
    }

    /// Get a reference to the underlying pool for direct access.
    pub fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    // -- Internal helpers -------------------------------------------------

    async fn query_rows_json(&self, sql: &str) -> Result<String, String> {
        let rows = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

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

        serde_json::to_string(&serde_json::json!({
            "rows": json_rows,
            "count": json_rows.len()
        }))
        .map_err(|e| e.to_string())
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
// ServiceProvider impl (raw access)
// ---------------------------------------------------------------------------

impl ServiceProvider for PostgresProvider {
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
            Err(e) => {
                serde_json::json!({ "error": e }).to_string().into_bytes()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PostgresHandle impl (typed API for WASM modules)
// ---------------------------------------------------------------------------

impl PostgresHandle for PostgresProvider {
    fn query(&self, sql: &str) -> Result<String, String> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(self.query_rows_json(sql))
    }

    fn execute(&self, sql: &str) -> Result<u64, String> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(self.execute_sql(sql))
    }

    fn query_with(&self, sql: &str, params: &[&str]) -> Result<String, String> {
        let rt = tokio::runtime::Handle::current();
        // Build a parameterised query
        let mut query = sqlx::query(sql);
        for p in params {
            query = query.bind(p);
        }
        rt.block_on(async {
            let rows = query
                .fetch_all(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
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
        })
    }
}
