# Micro-kernel Architecture — Tech Talk Demo

A **micro-kernel web backend** where the server core is minimal and all business
logic lives in dynamically-loaded WebAssembly modules. Modules can be deployed,
swapped, rolled back, call external services, and call each other.

> Built for the talk **"Building a Backend in Micro-kernel Architecture"**.

---

## Prerequisites

| Tool | Version | Check |
|------|---------|-------|
| Rust | 1.85+ (edition 2024) | `rustc --version` |
| WASM target | `wasm32-unknown-unknown` | `rustup target list --installed` |

```bash
rustup target add wasm32-unknown-unknown
```

## Environment Variables (optional)

The server runs without any database — all providers use echo fallbacks.
Set these to connect to real services:

```bash
export DATABASE_URL="postgres://localhost/testdb"
export MYSQL_URL="mysql://user:pass@localhost/db"
export REDIS_URL="redis://127.0.0.1:6379"
export S3_ENDPOINT="https://fra1.digitaloceanspaces.com"
export S3_KEY="YOUR_KEY"
export S3_SECRET="YOUR_SECRET"
```

---

## Getting Started

### 1. Build and run

```bash
cargo build
cargo run
```

The server starts on **`http://localhost:8080`**:

```
╔══════════════════════════════════════════════╗
║  Micro-kernel Architecture — Tech Talk Demo ║
╠══════════════════════════════════════════════╣
║  /wasm/              — example routes       ║
║  /user/*             — user module          ║
║  /order/*            — order module         ║
║  /dashboard          — module dashboard     ║
║  /api/...            — dashboard API        ║
╚══════════════════════════════════════════════╝
```

### 2. Open the dashboard

```
http://localhost:8080/dashboard
```

### 3. Test the endpoints

```bash
# Health
curl http://localhost:8080/wasm/health

# User module — typed service handles
curl http://localhost:8080/user/                # root
curl http://localhost:8080/user/list            # Postgres query (sqlx)
curl http://localhost:8080/user/cache           # Redis get (redis-rs)
curl http://localhost:8080/user/files           # S3 get (ureq)
curl http://localhost:8080/user/from-order      # calls Order module

# Order module
curl http://localhost:8080/order/               # root
curl http://localhost:8080/order/call-user      # calls User module
```

---

## Key Features

| Feature | For the talk |
|---------|-------------|
| **Typed service handles** | `pg.query()`, `redis.get()`, `s3.put()`, `http.get()` — full API per service |
| **Real providers** | `sqlx` (Postgres/MySQL), `redis-rs` (Redis), `ureq` (S3/HTTP) with fallback |
| **Blue-green deployment** | Dashboard — deploy v2, Swap, instant rollback |
| **Inter-module calls** | `call_module("user", "get_name", args)` with typed `FromModuleBytes` |
| **Middleware + Guards** | `WasmModule` trait — modules declare their own |
| **Graceful shutdown** | Dashboard → Server Control → Graceful or Force |

---

## Project Structure

```
wasm/
├── Cargo.toml              # Workspace root
├── README.md               # ← you are here
├── docs/                   # Full documentation (8 files)
├── modules/                # Drop .wasm files here
│
├── wasm-module/            # Module SDK crate (publishable to crates.io)
│   ├── Cargo.toml
│   ├── README.md
│   └── src/lib.rs          # WasmModule, ModuleContext, typed handles, etc.
│
└── server/                 # Micro-kernel runtime
    ├── Cargo.toml           # actix-web, wasmtime, sqlx, redis, ureq
    ├── static/dashboard.html
    └── src/
        ├── main.rs          # Entry point + demo modules
        ├── dashboard.rs     # Dashboard API + shutdown
        ├── scope.rs         # ModuleContext → Actix bridge
        ├── registry.rs      # Module registry (blue-green)
        ├── services.rs      # ServiceRegistry + ServiceProvider trait
        ├── providers/       # Real provider implementations
        │   ├── postgres.rs      # PostgresProvider (sqlx)
        │   ├── mysql.rs         # MySqlProvider (sqlx)
        │   ├── redis_provider.rs # RedisProvider (redis-rs)
        │   ├── s3.rs            # S3Provider (ureq)
        │   └── http_client.rs   # HttpProvider (ureq)
        ├── watcher.rs       # File watcher
        └── engine/          # wasmtime integration
```

---

## API Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/modules` | List modules (blue/green) |
| `POST` | `/api/modules/deploy` | Upload `.wasm` |
| `POST` | `/api/modules/{name}/swap` | Blue-green swap |
| `DELETE` | `/api/modules/{name}` | Remove module |
| `POST` | `/api/shutdown/graceful` | Graceful shutdown |
| `POST` | `/api/shutdown/force` | Force shutdown |
| `GET` | `/dashboard` | Dashboard UI |

---

## Documentation

```
docs/
├── README.md           # Overview & quick start
├── SUMMARY.md          # Deep-dive team overview (26 KB)
├── architecture.md     # Kernel internals, data flow, trait hierarchy
├── modules.md          # Module authoring — full API reference
├── services.md         # Providers — Postgres, Redis, MySQL, S3, HTTP
├── dashboard.md        # Dashboard UI guide
├── api.md              # REST API + shutdown endpoints
└── blue-green.md       # Deployment mechanism deep dive
```

---

## Testing

```bash
cargo test                        # 56 tests total
cargo build -p test-module --target wasm32-unknown-unknown
cargo test --test wasm_integration # WASM integration tests
```

| Layer | Tests |
|-------|-------|
| SDK (`wasm-module`) | 33 — traits, handlers, typed handles |
| Server unit | 17 — registry, services, exports, watcher |
| WASM integration | 3 — real .wasm module calls PG/Redis/S3/HTTP |

---

## Creating a Module

```rust
use wasm_module::{WasmModule, ModuleContext, Response};

struct MyModule;
impl WasmModule for MyModule {
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.export("hello");

        // Typed service handles — real APIs
        let pg = ctx.postgres.clone();

        ctx.get("/", || Response::ok("Hello!"))
           .get("/users", move || {
               let rows = pg.as_ref().unwrap()
                   .query("SELECT id, name FROM users")
                   .unwrap_or_default();
               Response::json(rows.into_bytes())
           });
    }

    fn on_export_call(&self, f: &str, _: &[u8]) -> Vec<u8> {
        match f { "hello" => b"Hello from module".to_vec(), _ => vec![] }
    }
}
```

See [docs/modules.md](docs/modules.md) for the full guide.
