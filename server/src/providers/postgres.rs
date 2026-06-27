//! Postgres Provider — sqlx-backed with OS-thread async execution.

use std::time::Duration;
use sqlx::{Column, Row};
use super::super::services::ServiceProvider;
use wasm_module::PostgresHandle;

#[derive(Debug, Clone)]
pub struct PostgresConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub max_lifetime: Option<Duration>,
    pub acquire_timeout: Option<Duration>,
    pub application_name: Option<String>,
    pub ssl_mode: Option<String>,
}
impl Default for PostgresConfig {
    fn default() -> Self {
        Self { url: String::new(), max_connections: 10, min_connections: 1,
            max_lifetime: Some(Duration::from_secs(1800)),
            acquire_timeout: Some(Duration::from_secs(10)),
            application_name: None, ssl_mode: None }
    }
}

pub struct PostgresProvider { pool: sqlx::PgPool }

impl PostgresProvider {
    pub async fn connect(config: PostgresConfig) -> Result<Self, sqlx::Error> {
        if config.url.is_empty() { return Err(sqlx::Error::Configuration("empty DATABASE_URL".into())); }
        let mut opts: sqlx::postgres::PgConnectOptions = config.url.parse()
            .map_err(|e| sqlx::Error::Configuration(Box::new(e)))?;
        if let Some(app) = &config.application_name { opts = opts.application_name(app); }
        if let Some(ssl) = &config.ssl_mode {
            opts = opts.ssl_mode(ssl.parse().map_err(|_| sqlx::Error::Configuration("invalid ssl_mode".into()))?);
        }
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections).min_connections(config.min_connections)
            .max_lifetime(config.max_lifetime)
            .acquire_timeout(config.acquire_timeout.unwrap_or(Duration::from_secs(10)))
            .connect_with(opts).await?;
        Ok(Self { pool })
    }
}

// -- ServiceProvider ---------------------------------------------------------

impl ServiceProvider for PostgresProvider {
    fn call(&self, _method: &str, payload: &[u8]) -> Vec<u8> {
        let body = String::from_utf8_lossy(payload).to_string();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::json!({"sql":body}));
        let sql = v["sql"].as_str().unwrap_or(&body).to_string();
        let kind = v["kind"].as_str().unwrap_or("query").to_string();
        let pool = self.pool.clone();
        let result: Result<String, String> = run_async(move || {
            let sql = sql.clone();
            async move {
                match kind.as_ref() {
                    "query" | "query_with" => {
                        let rows = sqlx::query(&sql).fetch_all(&pool).await.map_err(|e| e.to_string())?;
                        rows_to_json(&rows)
                    }
                    "execute" => {
                        let r = sqlx::query(&sql).execute(&pool).await.map_err(|e| e.to_string())?;
                        serde_json::to_string(&serde_json::json!({"rows_affected":r.rows_affected()})).map_err(|e| e.to_string())
                    }
                    _ => Err(format!("unknown kind: {kind}")),
                }
            }
        });
        match result { Ok(j) => j.into_bytes(), Err(e) => serde_json::json!({"error":e}).to_string().into_bytes() }
    }
}

// -- PostgresHandle ----------------------------------------------------------

impl PostgresHandle for PostgresProvider {
    fn query(&self, sql: &str) -> Result<String, String> {
        let pool = self.pool.clone(); let s = sql.to_string();
        run_async(move || async move {
            let rows = sqlx::query(&s).fetch_all(&pool).await.map_err(|e| e.to_string())?;
            rows_to_json(&rows)
        })
    }
    fn execute(&self, sql: &str) -> Result<u64, String> {
        let pool = self.pool.clone(); let s = sql.to_string();
        run_async(move || async move {
            let r = sqlx::query(&s).execute(&pool).await.map_err(|e| e.to_string())?;
            Ok(r.rows_affected())
        })
    }
    fn query_with(&self, sql: &str, params: &[&str]) -> Result<String, String> {
        let pool = self.pool.clone(); let s = sql.to_string();
        let p: Vec<String> = params.iter().map(|x| x.to_string()).collect();
        run_async(move || async move {
            let mut q = sqlx::query(&s);
            for x in &p { q = q.bind(x); }
            let rows = q.fetch_all(&pool).await.map_err(|e| e.to_string())?;
            rows_to_json(&rows)
        })
    }
}

// -- Helpers -----------------------------------------------------------------

fn run_async<F, Fut, T>(f: F) -> Result<T, String>
where F: FnOnce() -> Fut + Send + 'static,
      Fut: std::future::Future<Output = Result<T, String>> + Send,
      T: Send + 'static,
{
    std::thread::spawn(move || tokio::runtime::Runtime::new().unwrap().block_on(f()))
        .join().unwrap()
}

fn rows_to_json(rows: &[sqlx::postgres::PgRow]) -> Result<String, String> {
    let json_rows: Vec<serde_json::Value> = rows.iter().map(|row| {
        let mut map = serde_json::Map::new();
        for (i, col) in row.columns().iter().enumerate() {
            let val: String = row.try_get::<String, _>(i).unwrap_or_else(|_| "<unsupported>".into());
            map.insert(col.name().to_string(), serde_json::Value::String(val));
        }
        serde_json::Value::Object(map)
    }).collect();
    serde_json::to_string(&serde_json::json!({"rows":json_rows,"count":json_rows.len()})).map_err(|e| e.to_string())
}
