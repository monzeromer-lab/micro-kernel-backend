# Micro-kernel Architecture — Tech Talk Demo

A **micro-kernel web backend** where the server core (the "kernel") is minimal and all
business logic lives in dynamically-loaded WebAssembly modules. Modules can be deployed,
swapped, rolled back, call external services, and call each other — all without restarting
the server.

## Quick Start

```bash
# 1. Start the server
cargo run

# 2. Open the dashboard
open http://localhost:8080/dashboard

# 3. Hit the example endpoints
curl http://localhost:8080/wasm/
curl http://localhost:8080/wasm/health

# 4. Demo inter-module communication
curl http://localhost:8080/user/from-order      # user module calls order module
curl http://localhost:8080/order/call-user      # order module calls user module

# 5. Demo external service call
curl http://localhost:8080/user/list            # module calls Postgres via kernel
```

The server starts on **`http://localhost:8080`**. The dashboard is at **`/dashboard`**.

## What You'll See

| Path | What |
|------|------|
| `http://localhost:8080/dashboard` | Blue-green deployment dashboard |
| `http://localhost:8080/wasm/` | Example route |
| `http://localhost:8080/wasm/health` | Health check |
| `http://localhost:8080/user/` | User module root |
| `http://localhost:8080/user/list` | User module → calls Postgres |
| `http://localhost:8080/user/from-order` | User module → calls Order module |
| `http://localhost:8080/order/` | Order module root |
| `http://localhost:8080/order/call-user` | Order module → calls User module |
| `http://localhost:8080/api/modules` | Dashboard REST API |

## Key Features (for the talk)

| Feature | Where to demo |
|---------|--------------|
| **Dynamic module loading** | Drop a `.wasm` in `modules/`, watcher detects it |
| **Blue-green deployment** | Dashboard — deploy v2, hit Swap, instant rollback |
| **Inter-module calls** | `/user/from-order` calls Order module, `/order/call-user` calls User module |
| **External services** | `/user/list` calls Postgres through the kernel's `ServiceRegistry` |
| **Middleware + Guards** | `WasmModule` trait — modules declare their own |
| **Graceful shutdown** | Dashboard → Server Control → Graceful or Force |

## Project Layout

```
wasm/
├── Cargo.toml               # Workspace root
├── docs/                    # Documentation (you are here)
├── modules/                 # Drop .wasm files here
│
├── wasm-module/             # The module SDK crate
│   └── src/lib.rs           # WasmModule, ModuleContext, Response, ServiceRequirement, etc.
│
└── server/                  # The micro-kernel runtime
    ├── static/dashboard.html
    └── src/
        ├── main.rs          # Entry point — deploys demo modules
        ├── dashboard.rs     # Dashboard API + shutdown endpoints
        ├── scope.rs         # ModuleContext → Actix bridge
        ├── registry.rs      # Module registry with blue-green slots
        ├── services.rs      # ServiceRegistry — DB, HTTP, Redis + export registry
        ├── watcher.rs       # File-system watcher (notify)
        └── engine/          # wasmtime integration
```

## Further Reading

- [Architecture Deep Dive](architecture.md) — how the kernel works internally
- [Creating Modules](modules.md) — full API reference for module authors
- [Adding External Services](services.md) — how to add DB, HTTP, Redis, custom providers
- [Dashboard Guide](dashboard.md) — using the dashboard UI
- [API Reference](api.md) — REST API endpoints + shutdown
- [Blue-Green Deployment](blue-green.md) — deployment mechanism deep dive
