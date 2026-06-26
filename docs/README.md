# Micro-kernel Architecture — Tech Talk Demo

A demonstration of a **micro-kernel backend** where the server core (the "kernel") is
minimal — just an HTTP server and a WASM runtime. All business logic lives in
**dynamically-loaded WebAssembly modules** that can be deployed, swapped, and
rolled back without restarting the server.

## Quick Start

```bash
# 1. Start the server
cargo run

# 2. Open the dashboard
open http://localhost:8080/dashboard

# 3. Hit the example endpoints
curl http://localhost:8080/wasm/
curl http://localhost:8080/wasm/health
```

The server starts on **`http://localhost:8080`**. The dashboard is at **`/dashboard`**.

## What You'll See

| Path | What |
|------|------|
| `http://localhost:8080/dashboard` | Blue-green deployment dashboard |
| `http://localhost:8080/wasm/` | Example route (built with the module API) |
| `http://localhost:8080/wasm/health` | Health check endpoint |
| `http://localhost:8080/api/modules` | Dashboard REST API |

## Project Layout

```
wasm/
├── Cargo.toml               # Workspace root
├── docs/                    # Documentation (you are here)
├── modules/                 # Drop .wasm files here
│
├── wasm-module/             # The module SDK crate (what module authors use)
│   └── src/lib.rs           # WasmModule trait, ModuleContext, Response, Middleware, Guard
│
└── server/                  # The micro-kernel runtime
    ├── static/
    │   └── dashboard.html   # Dashboard UI
    └── src/
        ├── main.rs          # Entry point
        ├── dashboard.rs     # Dashboard API endpoints
        ├── scope.rs         # ModuleContext → Actix bridge
        ├── registry.rs      # Module registry with blue-green slots
        ├── watcher.rs       # File-system watcher (notify)
        └── engine/          # wasmtime integration
            ├── wasm_config.rs
            └── host_funcs.rs
```

## Further Reading

- [Architecture Deep Dive](architecture.md) — how the kernel works internally
- [Creating Modules](modules.md) — how to write a WASM module
- [Dashboard Guide](dashboard.md) — using the blue-green dashboard
- [API Reference](api.md) — REST API endpoints
- [Blue-Green Deployment](blue-green.md) — deep dive into the deployment mechanism
