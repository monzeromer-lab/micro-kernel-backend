# Adding External Services

There are two kinds of "external services" in this architecture. This guide
covers how to add both.

---

## Kind 1: Kernel-Level Service Providers

These are services the **kernel** manages — database pools, HTTP clients, Redis
connections, message queues, etc. Modules access them via `ctx.call_service()`.

### Architecture

```
Module                   Kernel                        External World
  │                        │                               │
  │  call_service(         │                               │
  │   "postgres","main_db",│                               │
  │   b"SELECT ...")       │                               │
  │ ──────────────────────▶│                               │
  │                        │  ServiceRegistry              │
  │                        │  "postgres/main_db"           │
  │                        │      │                        │
  │                        │      ▼                        │
  │                        │  PostgresProvider.call()      │
  │                        │      │                        │
  │                        │      │  pool.execute(sql) ───▶│  PostgreSQL
  │                        │      │                        │
  │                        │      │ ◀── rows ──────────────│
  │                        │      │                        │
  │  ◀── bytes ────────────│      │                        │
```

### Step 1: Implement `ServiceProvider`

```rust
// server/src/services.rs

use std::sync::Arc;

/// The trait every service provider must implement.
pub trait ServiceProvider: Send + Sync {
    /// Execute a call against this service.
    /// `method` is service-specific (SQL, URL, command name, etc.).
    fn call(&self, method: &str, payload: &[u8]) -> Vec<u8>;
}
```

#### Example: Real Postgres Provider (using sqlx)

```rust
use sqlx::postgres::PgPool;
use std::sync::Arc;

pub struct PostgresProvider {
    pool: PgPool,
}

impl PostgresProvider {
    pub async fn new(url: &str) -> Self {
        let pool = PgPool::connect(url).await
            .expect("failed to connect to Postgres");
        Self { pool }
    }
}

impl ServiceProvider for PostgresProvider {
    fn call(&self, _method: &str, payload: &[u8]) -> Vec<u8> {
        let sql = String::from_utf8_lossy(payload).to_string();

        // Use tokio runtime to block on async DB call
        let rt = tokio::runtime::Handle::current();
        let rows = rt.block_on(async {
            sqlx::query(&sql)
                .fetch_all(&self.pool)
                .await
        });

        match rows {
            Ok(rows) => {
                // Serialise rows to JSON
                let json_rows: Vec<serde_json::Value> = rows
                    .iter()
                    .map(|row| {
                        // Convert each row to a JSON object
                        // (simplified — real impl uses row columns)
                        serde_json::json!({"row": "data"})
                    })
                    .collect();
                serde_json::json!({"rows": json_rows}).to_string().into_bytes()
            }
            Err(e) => {
                serde_json::json!({"error": e.to_string()}).to_string().into_bytes()
            }
        }
    }
}
```

#### Example: Real HTTP Client Provider (using reqwest)

```rust
pub struct HttpClientProvider {
    client: reqwest::Client,
}

impl HttpClientProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl ServiceProvider for HttpClientProvider {
    fn call(&self, method: &str, payload: &[u8]) -> Vec<u8> {
        let url = String::from_utf8_lossy(payload).to_string();

        let rt = tokio::runtime::Handle::current();
        let result = rt.block_on(async {
            match method.to_lowercase().as_str() {
                "get" => self.client.get(&url).send().await,
                "post" => self.client.post(&url).body(url.clone()).send().await,
                _ => return b"{\"error\":\"unsupported method\"}".to_vec(),
            }
        });

        match result {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = rt.block_on(resp.text()).unwrap_or_default();
                format!(r#"{{"status":{},"body":"{}"}}"#, status, body).into_bytes()
            }
            Err(e) => {
                format!(r#"{{"error":"{}"}}"#, e).into_bytes()
            }
        }
    }
}
```

#### Example: Real Redis Provider (using redis-rs)

```rust
pub struct RedisProvider {
    client: redis::Client,
}

impl RedisProvider {
    pub fn new(url: &str) -> Self {
        let client = redis::Client::open(url)
            .expect("failed to connect to Redis");
        Self { client }
    }
}

impl ServiceProvider for RedisProvider {
    fn call(&self, method: &str, payload: &[u8]) -> Vec<u8> {
        let args = String::from_utf8_lossy(payload).to_string();

        let rt = tokio::runtime::Handle::current();
        let mut conn = rt.block_on(
            self.client.get_multiplexed_async_connection()
        ).expect("redis connection failed");

        let result: Result<String, _> = rt.block_on(async {
            match method.to_lowercase().as_str() {
                "get" => redis::cmd("GET").arg(&args).query_async(&mut conn).await,
                "set" => {
                    let parts: Vec<&str> = args.splitn(2, ' ').collect();
                    redis::cmd("SET")
                        .arg(parts[0])
                        .arg(parts.get(1).unwrap_or(&""))
                        .query_async(&mut conn).await
                }
                "del" => redis::cmd("DEL").arg(&args).query_async(&mut conn).await,
                _ => return b"{\"error\":\"unsupported command\"}".to_vec(),
            }
        });

        match result {
            Ok(val) => format!(r#"{{"result":"{}"}}"#, val).into_bytes(),
            Err(e) => format!(r#"{{"error":"{}"}}"#, e).into_bytes(),
        }
    }
}
```

### Step 2: Register with the Kernel

In `server/src/main.rs`, after creating the `ServiceRegistry`:

```rust
// Create the service provider at startup
let pg_pool = PgPool::connect("postgres://localhost/mydb").await?;
let pg_provider = PostgresProvider { pool: pg_pool };

// Register it with the kernel
service_registry.register_service(
    "postgres",        // kind (must match what modules use)
    "main_db",         // identifier (must match what modules use)
    pg_provider,
);

// Register more providers
service_registry.register_service("http", "default", HttpClientProvider::new());
service_registry.register_service("redis", "cache", RedisProvider::new("redis://localhost"));

// Custom providers — any kind/identifier pair works
service_registry.register_service("grpc", "user_service", GrpcProvider::new(addr));
service_registry.register_service("kafka", "events", KafkaProvider::new(brokers));
```

### Step 3: Use from a Module

```rust
impl WasmModule for MyModule {
    fn properties(&self) -> ModuleProperties {
        ModuleProperties {
            required_services: vec![
                ServiceRequirement { kind: ServiceKind::Postgres, identifier: "main_db".into() },
                ServiceRequirement { kind: ServiceKind::Http, identifier: "default".into() },
                ServiceRequirement { kind: ServiceKind::Redis, identifier: "cache".into() },
            ],
            ..Default::default()
        }
    }

    fn register(&self, ctx: &mut ModuleContext) {
        let call_svc = ctx.call_service.clone();

        ctx.get("/db", move || {
            let rows = call_svc.as_ref().unwrap()("postgres", "main_db", b"SELECT 1");
            Response::json(rows)
        })
        .get("/fetch", move || {
            let data = call_svc.as_ref().unwrap()("http", "default", b"https://api.example.com");
            Response::json(data)
        })
        .get("/cache", move || {
            let val = call_svc.as_ref().unwrap()("redis", "cache", b"GET mykey");
            Response::ok(val)
        });
    }
}
```

### Adding a Custom Service Kind

The `ServiceKind` enum in `wasm-module/src/lib.rs` defines recognised kinds:

```rust
pub enum ServiceKind {
    Postgres,
    Http,
    Redis,
    // Add new kinds here:
    // Grpc,
    // Kafka,
    // S3,
}
```

For completely custom providers, modules can use any `kind` string — the enum
is just for validation in `properties()`. The kernel matches on the string key
`"kind/identifier"`.

### Built-in Demo Providers

The demo includes three providers that echo back what they receive (no real
connections needed):

| Provider | What it does |
|----------|-------------|
| `PostgresProvider` | Logs SQL, returns `{"rows":[],"service":"postgres/main"}` |
| `HttpClientProvider` | Logs request, echoes body back |
| `RedisProvider` | Logs command, returns `{"result":"ok"}` |

---

## Kind 2: HTTP-Based Services (Other WASM Modules)

Any WASM module deployed in the system is already an HTTP service. Other modules
can interact with it in two ways.

### Option A: Inter-Module Calls (`call_module`)

This is the **direct** way — Module B calls Module A's exported function without
going through HTTP.

```rust
// Module B calls Module A's export
fn register(&self, ctx: &mut ModuleContext) {
    let call_mod = ctx.call_module.clone();

    ctx.get("/call-a", move || {
        let result = call_mod.as_ref().unwrap()(
            "module_a",      // module name
            "get_name",      // exported function
            b"{}"            // arguments
        );
        Response::json(result)
    });
}
```

**When to use**: Same-process, low-latency, no serialisation overhead.
The kernel routes the call directly to `ModuleA.on_export_call()`.

### Option B: HTTP Calls (via `call_service`)

Module B can call Module A's HTTP endpoints the same way it calls any external
API — through the kernel's HTTP client provider.

```rust
fn register(&self, ctx: &mut ModuleContext) {
    let call_svc = ctx.call_service.clone();

    ctx.get("/call-a-http", move || {
        let result = call_svc.as_ref().unwrap()(
            "http",           // use the HTTP provider
            "default",        // provider identifier
            b"http://localhost:8080/module_a/some-endpoint"
        );
        Response::json(result)
    });
}
```

**When to use**: When modules are on different hosts, when you need standard
HTTP semantics (status codes, headers, caching), or when the called module
expects HTTP-formatted input.

### Comparison

| Aspect | `call_module` (direct) | HTTP via `call_service` |
|--------|----------------------|------------------------|
| Transport | In-memory function call | HTTP request |
| Latency | Microseconds | Milliseconds |
| Serialisation | Raw bytes (your choice) | HTTP body (your choice) |
| Works across hosts | No (same process) | Yes |
| Works across languages | Only Rust/WASM | Any language |
| Status codes | Manual in bytes | HTTP response has them |
| Middleware | Not applicable | Actix middleware applies |

### Example: Building a "Payment Module" as an HTTP Service

```rust
struct PaymentModule;

impl WasmModule for PaymentModule {
    fn register(&self, ctx: &mut ModuleContext) {
        // Standard HTTP endpoints — any HTTP client can call these
        ctx.post("/charge", || Response::json(b"{\"status\":\"charged\"}".to_vec()));
        ctx.get("/status/:id", || Response::ok("payment status"));
        ctx.post("/refund", || Response::json(b"{\"status\":\"refunded\"}".to_vec()));
    }
}
```

Any other module (or external client) can now call:

```bash
curl -X POST http://localhost:8080/payment/charge
curl http://localhost:8080/payment/status/abc123
```

Or from another module:

```rust
// Via inter-module call
call_mod("payment", "process", b"{\"amount\":100}");

// Via HTTP
call_svc("http", "default", b"POST /payment/charge {\"amount\":100}");
```

### Example: Module as a gRPC-Like Service

You can also build specialised protocols on top of `call_module`:

```rust
// Module A: gRPC-like service
fn on_export_call(&self, function: &str, args: &[u8]) -> Vec<u8> {
    match function {
        "UserService.GetUser" => {
            let req: GetUserRequest = serde_json::from_slice(args).unwrap();
            let user = self.db.find_user(req.id);
            serde_json::to_vec(&GetUserResponse { user }).unwrap()
        }
        _ => vec![],
    }
}

// Module B: gRPC-like client
fn register(&self, ctx: &mut ModuleContext) {
    let call_mod = ctx.call_module.clone();

    ctx.get("/user/:id", move || {
        let req = serde_json::to_vec(&GetUserRequest { id: 42 }).unwrap();
        let resp_bytes = call_mod.as_ref().unwrap()(
            "user_service", "UserService.GetUser", &req
        );
        let user: GetUserResponse = serde_json::from_slice(&resp_bytes).unwrap();
        Response::json(serde_json::to_vec(&user).unwrap())
    });
}
```

### Summary

```
┌─────────────────────────────────────────────────────────┐
│  How modules talk to other services                      │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │  Kernel-Level Services (ServiceProvider trait)   │   │
│  │  • Postgres, HTTP, Redis, Kafka, S3, etc.        │   │
│  │  • Accessed via: call_service(kind, id, payload)  │   │
│  │  • Registered at kernel startup                  │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │  Other WASM Modules                               │   │
│  │  ┌─────────────────┐  ┌──────────────────────┐   │   │
│  │  │ call_module()    │  │ call_service("http") │   │   │
│  │  │ Direct, in-proc  │  │ Standard HTTP        │   │   │
│  │  │ (microseconds)   │  │ (milliseconds)       │   │   │
│  │  └─────────────────┘  └──────────────────────┘   │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```
