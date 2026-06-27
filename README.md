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
# Example scope
curl http://localhost:8080/wasm/health          # → {"status":"ok"}

# User module
curl http://localhost:8080/user/                # → "User Module"
curl http://localhost:8080/user/list            # → calls Postgres via kernel
curl http://localhost:8080/user/from-order      # → calls Order module

# Order module
curl http://localhost:8080/order/               # → "Order Module"
curl http://localhost:8080/order/call-user      # → calls User module
```

---

## Key Features

| Feature | For the talk |
|---------|-------------|
| Dynamic module loading | Drop `.wasm` in `modules/`, watcher picks it up |
| Blue-green deployment | Dashboard — deploy v2, Swap, instant rollback |
| Inter-module calls | `/user/from-order` and `/order/call-user` |
| External services | `/user/list` calls Postgres through kernel |
| Middleware + Guards | Built into the `WasmModule` trait |
| Graceful shutdown | Dashboard → Server Control |

---

## Project Structure

```
wasm/
├── Cargo.toml              # Workspace root
├── README.md               # ← you are here
├── docs/                   # Full documentation
│   ├── README.md
│   ├── architecture.md     # Inner workings, data flow
│   ├── modules.md          # Module creation guide
│   ├── services.md         # Adding external services
│   ├── dashboard.md        # Dashboard UI guide
│   ├── api.md              # REST API reference
│   └── blue-green.md       # Deployment mechanism deep dive
├── modules/                # Drop .wasm files here
│
├── wasm-module/            # Module SDK crate (publishable)
│   ├── Cargo.toml
│   ├── README.md
│   └── src/lib.rs
│
└── server/                 # Micro-kernel runtime
    ├── Cargo.toml
    ├── static/dashboard.html
    └── src/
        ├── main.rs
        ├── dashboard.rs
        ├── scope.rs
        ├── registry.rs
        ├── services.rs       # ServiceRegistry
        ├── watcher.rs
        └── engine/
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

```bash
docs/
├── README.md           # Overview & quick start
├── architecture.md     # How the kernel works
├── modules.md          # Writing modules (full API reference)
├── services.md         # Adding DB, HTTP, Redis, custom providers
├── dashboard.md        # Using the dashboard
├── api.md              # REST API endpoints
└── blue-green.md       # Blue-green deployment deep dive
```

---

## Creating a Module

```rust
use wasm_module::{WasmModule, ModuleContext, Response};

struct MyModule;
impl WasmModule for MyModule {
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.export("hello")
           .get("/", || Response::ok("Hello!"));
    }
    fn on_export_call(&self, f: &str, _: &[u8]) -> Vec<u8> {
        match f { "hello" => b"Hello from module".to_vec(), _ => vec![] }
    }
}
```

See [docs/modules.md](docs/modules.md) for the full guide.
