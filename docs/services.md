# External Services

Modules access databases, caches, object storage, and HTTP APIs through **typed
service handles** provided by the kernel. Each handle wraps a real backend
implementation.

---

## Available Providers

| Provider | Backend | Config struct | Typed trait |
|----------|---------|--------------|-------------|
| **Postgres** | `sqlx::PgPool` | `PostgresConfig` | `PostgresHandle` |
| **MySQL** | `sqlx::MySqlPool` | `MySqlConfig` | `MySqlHandle` |
| **Redis** | `redis::Connection` | `RedisConfig` | `RedisHandle` |
| **S3** | `ureq::Agent` | `S3Config` | `S3Handle` |
| **HTTP** | `ureq::Agent` | `HttpConfig` | `HttpHandle` |

---

## Environment Variables

All providers read configuration from environment variables at startup.
When env vars are missing or connections fail, **echo fallback providers**
are registered — the server always starts.

| Variable | Provider | Example |
|----------|----------|---------|
| `DATABASE_URL` | Postgres | `postgres://localhost/testdb` |
| `MYSQL_URL` | MySQL | `mysql://user:pass@localhost/db` |
| `REDIS_URL` | Redis | `redis://127.0.0.1:6379` |
| `S3_ENDPOINT` | S3 | `https://fra1.digitaloceanspaces.com` |
| `S3_KEY` | S3 | Your access key |
| `S3_SECRET` | S3 | Your secret key |
| `S3_REGION` | S3 | `fra1` (default: `us-east-1`) |

## Async Execution (Postgres & MySQL)

Postgres and MySQL use `sqlx` which requires an async runtime. Since Actix
handlers run on single-threaded arbiters that can't block, the providers
**spawn OS threads** to execute queries:

```
Module handler (actix thread)
    │
    ▼
pg.query("SELECT ...")  ← PostgresHandle trait
    │
    ▼
std::thread::spawn(|| {           ← fresh OS thread
    tokio::runtime::Runtime::new()  ← fresh tokio runtime
        .block_on(sqlx::query(...))  ← async DB call
})
    │
    ▼
Result returned to actix thread
```

This ensures the actix worker thread is never blocked while a database
query runs. Redis and S3/HTTP providers are synchronous (no async needed).

---

## How Modules Use Them

```rust
impl WasmModule for MyModule {
    fn register(&self, ctx: &mut ModuleContext) {
        // Clone typed handles before mutable borrows
        let pg = ctx.postgres.clone();
        let redis = ctx.redis.clone();
        let s3 = ctx.s3.clone();
        let http = ctx.http.clone();

        ctx.get("/users", move || {
            let rows = pg.as_ref().unwrap()
                .query("SELECT id, name FROM users WHERE active = true")
                .unwrap_or_else(|e| format!(r#"{{"error":"{}"}}"#, e));
            Response::json(rows.into_bytes())
        })
        .get("/cache", move || {
            let val = redis.as_ref().unwrap()
                .get("homepage:stats").unwrap()
                .unwrap_or_else(|| "not cached".into());
            Response::ok(val)
        })
        .get("/files", move || {
            let data = s3.as_ref().unwrap()
                .get("my-bucket", "hello.txt")
                .unwrap_or_else(|e| e.as_bytes().to_vec());
            Response::ok(String::from_utf8_lossy(&data).to_string())
        })
        .get("/external", move || {
            let body = http.as_ref().unwrap()
                .get("https://api.github.com/zen")
                .unwrap_or_else(|e| e);
            Response::ok(body)
        });
    }

    fn properties(&self) -> ModuleProperties {
        ModuleProperties {
            required_services: vec![
                ServiceRequirement { kind: ServiceKind::Postgres, identifier: "main_db".into() },
                ServiceRequirement { kind: ServiceKind::Redis, identifier: "cache".into() },
                ServiceRequirement { kind: ServiceKind::S3, identifier: "assets".into() },
            ],
            ..Default::default()
        }
    }
}
```

---

## Architecture

```
Module handler:  pg.query("SELECT ...")
                    │
                    ▼
ServiceRegistryPostgresHandle.query()
   (thin wrapper in kernel — delegates to ServiceRegistry)
                    │
                    ▼
ServiceRegistry.call_service("postgres", "main_db", json)
                    │
                    ▼
PostgresProvider.call()
   (real sqlx execution: pool.execute(sql).await)
                    │
                    ▼
Returns JSON rows → back to module
```

The kernel creates provider instances at startup, registers them in the
`ServiceRegistry`, and wraps them in thin `ServiceRegistryXxxHandle` structs
that are assigned to `ModuleContext.postgres`, `.redis`, etc.

---

## Postgres Provider

```rust
// server/src/providers/postgres.rs

pub struct PostgresConfig {
    pub url: String,                         // postgres://user:pass@host/db
    pub max_connections: u32,                // default: 10
    pub min_connections: u32,                // default: 1
    pub max_lifetime: Option<Duration>,      // default: 30 min
    pub acquire_timeout: Option<Duration>,   // default: 10 s
    pub application_name: Option<String>,
    pub ssl_mode: Option<String>,            // "disable", "require", etc.
}
```

### Typed Handle Methods

| Method | Returns | Purpose |
|--------|---------|---------|
| `query(sql)` | `Result<String>` | SELECT → JSON rows string |
| `execute(sql)` | `Result<u64>` | INSERT/UPDATE/DELETE → rows affected |
| `query_with(sql, params)` | `Result<String>` | Parameterised query ($1, $2, ...) |

### Registering at Startup

```rust
let pg = PostgresProvider::connect(PostgresConfig {
    url: std::env::var("DATABASE_URL").unwrap(),
    max_connections: 20,
    ..Default::default()
}).await?;

service_registry.register_service("postgres", "main_db", pg);
```

If the connection fails, an `EchoProvider` fallback is registered so the demo
still runs without a real database.

---

## MySQL Provider

```rust
pub struct MySqlConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub max_lifetime: Option<Duration>,
    pub acquire_timeout: Option<Duration>,
}
```

Same typed handle methods as Postgres: `query()`, `execute()`, `query_with()`.

---

## Redis Provider

```rust
pub struct RedisConfig {
    pub url: String,                         // redis://[:password@]host:port[/db]
    pub connection_timeout: Option<Duration>, // default: 5 s
    pub default_ttl_seconds: Option<u64>,     // auto-EXPIRE on set()
    pub key_prefix: Option<String>,           // namespace all keys
}
```

### Typed Handle Methods

| Method | Returns | Purpose |
|--------|---------|---------|
| `get(key)` | `Result<Option<String>>` | Get a key |
| `set(key, value, ttl)` | `Result<()>` | Set with optional TTL |
| `del(keys)` | `Result<u64>` | Delete keys, returns count |
| `incr(key, amount)` | `Result<i64>` | Increment, returns new value |
| `exists(key)` | `Result<bool>` | Check existence |

---

## S3 Provider

```rust
pub struct S3Config {
    pub endpoint: String,          // https://s3.amazonaws.com or http://localhost:9000
    pub region: String,            // us-east-1
    pub access_key: String,        // AKIA...
    pub secret_key: String,        // ...
    pub session_token: Option<String>,
    pub force_path_style: bool,    // true for MinIO, false for AWS
    pub timeout: Option<Duration>,
}
```

Works with AWS S3, MinIO, CloudFlare R2, DigitalOcean Spaces, etc.

### Typed Handle Methods

| Method | Returns | Purpose |
|--------|---------|---------|
| `put(bucket, key, data)` | `Result<String>` | Upload object |
| `get(bucket, key)` | `Result<Vec<u8>>` | Download object |
| `delete(bucket, key)` | `Result<bool>` | Delete object |
| `list(bucket, prefix)` | `Result<String>` | List objects (XML response) |

---

## HTTP Provider

```rust
pub struct HttpConfig {
    pub user_agent: String,
    pub timeout: Option<Duration>,
    pub connect_timeout: Option<Duration>,
    pub danger_accept_invalid_certs: bool,
}
```

### Typed Handle Methods

| Method | Returns | Purpose |
|--------|---------|---------|
| `get(url)` | `Result<String>` | GET request → response body |
| `post(url, body)` | `Result<String>` | POST request |
| `put(url, body)` | `Result<String>` | PUT request |
| `delete(url)` | `Result<String>` | DELETE request |

---

## Adding a New Provider

### 1. Add the typed trait to `wasm-module/src/lib.rs`

```rust
pub trait KafkaHandle: Send + Sync {
    fn publish(&self, topic: &str, message: &[u8]) -> Result<(), String>;
    fn subscribe(&self, topic: &str) -> Result<String, String>;
}
```

### 2. Add the field to `ModuleContext`

```rust
pub struct ModuleContext {
    // ... existing fields ...
    pub kafka: Option<Arc<dyn KafkaHandle>>,
}
```

### 3. Implement the provider in `server/src/providers/`

```rust
pub struct KafkaProvider { /* rdkafka producer */ }

impl ServiceProvider for KafkaProvider { ... }
impl KafkaHandle for KafkaProvider { ... }
```

### 4. Register at startup in `main.rs`

```rust
service_registry.register_service("kafka", "events", kafka_provider);
```

### 5. Add a thin handle wrapper in `main.rs`

```rust
struct ServiceRegistryKafkaHandle(Arc<Mutex<ServiceRegistry>>);
impl KafkaHandle for ServiceRegistryKafkaHandle { ... }

// In build_module_context:
ctx.kafka = Some(Arc::new(ServiceRegistryKafkaHandle(Arc::clone(&svc))));
```

### 6. Update `ServiceKind` enum if wanted

```rust
pub enum ServiceKind {
    Postgres, Http, Redis, MySql, S3,
    Kafka, // NEW
}
```

---

## ModuleContext Handle Fields

All handle fields are `Option<Arc<dyn Trait>>` — set by the host before `register()`:

```rust
pub struct ModuleContext {
    pub postgres: Option<Arc<dyn PostgresHandle>>,
    pub redis:    Option<Arc<dyn RedisHandle>>,
    pub mysql:    Option<Arc<dyn MySqlHandle>>,
    pub s3:       Option<Arc<dyn S3Handle>>,
    pub http:     Option<Arc<dyn HttpHandle>>,

    // Raw callbacks (also available):
    pub call_service: Option<Arc<ServiceCallFn>>,
    pub call_module:  Option<Arc<ModuleCallFn>>,
}
```

All are propagated to nested scopes automatically.
