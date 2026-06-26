# Micro-kernel Architecture — Tech Talk Demo

A **micro-kernel web backend** where the server core is minimal and all
business logic lives in dynamically-loaded WebAssembly modules. Modules
can be deployed, swapped, and rolled back without restarting the server.

> Built for the talk **"Building a Backend in Micro-kernel Architecture"**.

---

## Prerequisites

| Tool | Version | Check |
|------|---------|-------|
| Rust | 1.85+ (edition 2024) | `rustc --version` |
| WASM target | `wasm32-unknown-unknown` | `rustup target list --installed` |

### Install the WASM target

```bash
rustup target add wasm32-unknown-unknown
```

---

## Getting Started

### 1. Clone and build

```bash
git clone <repo-url>
cd wasm
cargo build
```

### 2. Start the server

```bash
cargo run
```

The server starts on **`http://localhost:8080`** and prints:

```
╔══════════════════════════════════════════════╗
║  Micro-kernel Architecture — Tech Talk Demo ║
╠══════════════════════════════════════════════╣
║  /wasm/              — example routes       ║
║  /dashboard          — module dashboard     ║
║  /api/modules        — dashboard API        ║
║  Module folder: ./modules/                  ║
╚══════════════════════════════════════════════╝
[kernel] wasmtime engine ready
[watcher] watching ./modules/ for .wasm files...
```

### 3. Open the dashboard

```
http://localhost:8080/dashboard
```

### 4. Test the example endpoints

```bash
curl http://localhost:8080/wasm/
# → "Hello from the dynamic scope!"

curl http://localhost:8080/wasm/health
# → {"status":"ok"}
```

---

## Creating a Module

### 1. Add the dependency

```toml
# your-module/Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
wasm-module = "0.1"
```

### 2. Implement the trait

```rust
use wasm_module::{WasmModule, ModuleContext, Response};

struct MyModule;

impl WasmModule for MyModule {
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.get("/", || Response::ok("Hello from my module!"));
        ctx.get("/data", || Response::json(b"[1,2,3]".to_vec()));
    }
}
```

### 3. Compile to WASM

```bash
cargo build --target wasm32-unknown-unknown --release
```

### 4. Deploy

Drop the `.wasm` file into the `modules/` folder:

```bash
cp target/wasm32-unknown-unknown/release/my_module.wasm ./modules/
```

The file watcher detects it. The module's name is taken from the filename
stem (e.g. `my_module.wasm` → mounted at `/my_module/...`).

> **Naming rules**: lowercase a–z only. No numbers, no special characters.
> `user.wasm` ✓ — `user_api.wasm` ✗ — `User.wasm` ✗ — `user1.wasm` ✗

---

## Dashboard

```
http://localhost:8080/dashboard
```

| Feature | How |
|---------|-----|
| **View modules** | See all deployed modules with blue/green slot status |
| **Deploy** | Click "Deploy Module" → select a `.wasm` file |
| **Swap** | Click "Swap" to instantly switch blue ↔ green |
| **Remove** | Click "Remove" to delete a module |

See [docs/dashboard.md](docs/dashboard.md) for the full UI guide.

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
│   ├── dashboard.md        # Dashboard UI guide
│   ├── api.md              # REST API reference
│   └── blue-green.md       # Deployment mechanism deep dive
├── modules/                # Drop .wasm files here
│
├── wasm-module/            # The module SDK crate (publishable)
│   ├── Cargo.toml
│   ├── README.md
│   └── src/lib.rs          # WasmModule, ModuleContext, Response, etc.
│
└── server/                 # The micro-kernel runtime
    ├── Cargo.toml
    ├── static/
    │   └── dashboard.html
    └── src/
        ├── main.rs
        ├── dashboard.rs
        ├── scope.rs
        ├── registry.rs
        ├── watcher.rs
        └── engine/
            ├── mod.rs
            ├── wasm_config.rs
            └── host_funcs.rs
```

---

## API Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/modules` | List all modules (blue/green status) |
| `POST` | `/api/modules/deploy` | Upload a `.wasm` module |
| `POST` | `/api/modules/{name}/swap` | Swap blue ↔ green |
| `DELETE` | `/api/modules/{name}` | Remove a module |
| `GET` | `/dashboard` | Dashboard HTML UI |
| `GET` | `/wasm/` | Example route |
| `GET` | `/wasm/health` | Health check |

See [docs/api.md](docs/api.md) for full API reference with curl examples.

---

## Documentation

```bash
docs/
├── README.md           # Overview & quick start
├── architecture.md     # How the kernel works
├── modules.md          # Writing modules (full API reference)
├── dashboard.md        # Using the dashboard
├── api.md              # REST API endpoints
└── blue-green.md       # Blue-green deployment deep dive
```

---

## License

MIT OR Apache-2.0 — this is a tech talk demo.
