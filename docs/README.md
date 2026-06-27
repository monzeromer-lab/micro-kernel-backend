# Micro-kernel Architecture — Tech Talk Demo

A **micro-kernel web backend** where the server core (the "kernel") is minimal and all
business logic lives in dynamically-loaded WebAssembly modules.

## Quick Start

```bash
cargo run
open http://localhost:8080/dashboard

# Demo endpoints
curl http://localhost:8080/wasm/health
curl http://localhost:8080/user/list           # Postgres via kernel
curl http://localhost:8080/user/cache          # Redis via kernel
curl http://localhost:8080/user/files          # S3 via kernel
curl http://localhost:8080/user/from-order     # inter-module call
curl http://localhost:8080/order/call-user     # inter-module call
```

## Endpoints

| Path | What |
|------|------|
| `/dashboard` | Blue-green deployment dashboard |
| `/wasm/health` | Health check |
| `/user/list` | Postgres query (typed `pg.query()`) |
| `/user/cache` | Redis get (typed `redis.get()`) |
| `/user/files` | S3 get (typed `s3.get()`) |
| `/user/from-order` | Inter-module: User → Order |
| `/order/call-user` | Inter-module: Order → User |
| `/api/modules` | Dashboard REST API |

## Key Features

| Feature | Demo |
|---------|------|
| **Typed service handles** | `pg.query()`, `redis.get()`, `s3.put()`, `http.get()` — full API per service |
| **Real providers** | `sqlx` (Postgres/MySQL), `redis-rs`, `ureq` (S3/HTTP) |
| **Blue-green deployment** | Dashboard — deploy v2, Swap, instant rollback |
| **Inter-module calls** | `call_module("user", "get_name", args)` |
| **Middleware + Guards** | `WasmModule` trait |
| **Graceful shutdown** | Dashboard → Server Control |

## Project Layout

```
wasm/
├── docs/                    # Documentation
├── modules/                 # Drop .wasm files here
├── wasm-module/             # Module SDK crate (publishable)
│   └── src/lib.rs           # WasmModule, ModuleContext, typed handles, etc.
└── server/                  # Micro-kernel runtime
    ├── static/dashboard.html
    └── src/
        ├── main.rs          # Entry point + demo modules
        ├── dashboard.rs     # Dashboard API
        ├── scope.rs         # ModuleContext → Actix bridge
        ├── registry.rs      # Module registry (blue-green)
        ├── services.rs      # ServiceRegistry + ServiceProvider trait
        ├── providers/       # Real provider implementations
        │   ├── postgres.rs  #   PostgresProvider (sqlx)
        │   ├── mysql.rs     #   MySqlProvider (sqlx)
        │   ├── redis_provider.rs # RedisProvider (redis-rs)
        │   ├── s3.rs        #   S3Provider (ureq)
        │   └── http_client.rs   # HttpProvider (ureq)
        ├── watcher.rs       # File watcher
        └── engine/          # wasmtime integration
```

## Further Reading

- [Architecture](architecture.md) — kernel internals, data flow
- [Creating Modules](modules.md) — full API reference
- [External Services](services.md) — provider guide, adding new ones
- [Dashboard](dashboard.md) — UI guide
- [API Reference](api.md) — REST endpoints
- [Blue-Green](blue-green.md) — deployment mechanism
- [Summary](SUMMARY.md) — team overview
