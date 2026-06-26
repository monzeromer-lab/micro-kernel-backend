# Architecture

## The Micro-kernel Concept

A **micro-kernel** operating system has a tiny core that does only the bare minimum —
memory management, process scheduling, IPC — and pushes everything else into
user-space processes. This project applies the same idea to a web backend:

```
┌─────────────────────────────────────────────────┐
│                    KERNEL                        │
│  ┌──────────┐  ┌──────────┐  ┌───────────────┐  │
│  │  Actix   │  │ wasmtime │  │   Module      │  │
│  │  HTTP    │  │  Engine  │  │   Registry    │  │
│  └──────────┘  └──────────┘  └───────────────┘  │
│                                                  │
│  The kernel ONLY does:                           │
│    • HTTP routing                                │
│    • WASM compilation & instantiation            │
│    • Module lifecycle (load/unload/swap)         │
└─────────────────────────────────────────────────┘
                        │
          ┌─────────────┼─────────────┐
          ▼             ▼             ▼
     ┌────────┐   ┌────────┐   ┌────────┐
     │ user.  │   │ order. │   │ payment│
     │ wasm   │   │ wasm   │   │ .wasm  │
     └────────┘   └────────┘   └────────┘

     Each module is an independent WASM binary.
     It registers its own routes, middleware, guards.
     Modules don't know about each other.
```

## Data Flow (Request → Response)

```
1. HTTP Request arrives at Actix
         │
2. Actix dispatches to matching route
         │
3. Route was registered by a WASM module (via ModuleContext)
         │
4. Handler runs:
   - If native: closure returns Response immediately
   - If WASM: host calls into wasmtime Func → guest code runs → returns via memory
         │
5. rayna_module::Response → converted to actix_web::HttpResponse
         │
6. Response sent to client
```

## Component Map

### Two Crates (Workspace)

| Crate | Purpose | Dependencies |
|-------|---------|-------------|
| `wasm-module` | The **contract** that module authors implement. Defines traits and data types. | Zero heavy deps. Only `std`. |
| `wasm-server` | The **kernel** that loads and runs modules. Provides Actix server, wasmtime engine, dashboard. | `actix-web`, `wasmtime`, `notify`, `tokio` |

### Why Two Crates?

`wasm-module` is **publishable to crates.io** independently. A module author adds:

```toml
[dependencies]
wasm-module = "0.1"
```

And implements the `WasmModule` trait — no need to pull in `actix-web` or `wasmtime`.

### Key Data Structures

#### ModuleContext (in `wasm-module`)

```rust
pub struct ModuleContext {
    routes: Vec<RouteDef>,        // GET /path → Handler
    scopes: Vec<ScopeDef>,        // nested /prefix → sub-ModuleContext
    middleware: Vec<Box<dyn Middleware>>,
    guards: Vec<Box<dyn Guard>>,
}
```

Built by the module during `register()`. The kernel reads it and converts to Actix routes.

#### ModuleRegistry (in `wasm-server`)

```rust
pub struct ModuleRegistry {
    modules: HashMap<String, ModuleSlots>,
}

pub struct ModuleSlots {
    pub active: String,           // "blue" or "green"
    pub blue: Option<ModuleEntry>,
    pub green: Option<ModuleEntry>,
}
```

Each module name maps to two deployment slots. Only the `active` slot serves traffic.

### Trait Hierarchy

```
WasmModule  ←── every module implements this
    │
    ├── register(&self, ctx: &mut ModuleContext)
    │       │
    │       ├── ctx.get(path, handler)
    │       ├── ctx.post(path, handler)
    │       ├── ctx.scope(prefix, |sub| { ... })
    │       ├── ctx.middleware(mw)
    │       └── ctx.guard(g)
    │
    ├── properties() → ModuleProperties
    │       memory_pages, max_memory_pages, memory64, consume_fuel
    │
    └── version() → (u16, u16, u16)

Middleware  ←── request/response interceptors
    ├── name() → &str
    ├── before() → bool
    └── after() → bool

Guard  ←── conditional routing gates
    ├── name() → &str
    └── check() → bool

Handler  ←── route callbacks
    └── call() → Response
```

## File Watcher (notify)

The watcher monitors `./modules/` for `.wasm` file changes using the `notify` crate:

```
modules/
├── user.wasm       ← detected → name = "user" → mount at /user/*
├── product.wasm    ← detected → name = "product" → mount at /product/*
```

**Naming rules**: lowercase a–z only. No numbers, no special characters, no underscores.
The filename stem becomes the URL prefix. `user.wasm` → `/user/...`.

Events:
- **Create** → module added (TODO: auto-compile & register)
- **Modify** → module updated (TODO: auto-redeploy into inactive slot)
- **Remove** → module removed from registry

## wasmtime Engine Configuration

The kernel creates a single `wasmtime::Engine` at startup with these defaults:

```rust
config.wasm_bulk_memory(true);     // efficient memory operations
config.wasm_multi_value(true);     // multiple return values
config.wasm_multi_memory(true);    // multiple memories
config.wasm_reference_types(true); // externref/funcref
config.wasm_simd(true);            // SIMD instructions
config.cranelift_opt_level(Speed); // optimise for runtime speed
config.epoch_interruption(true);   // can cancel runaway modules
```

Each module can override settings via `ModuleProperties` returned by `WasmModule::properties()`.

## Actix Integration

The `scope::mount_context()` function in `wasm-server` is the bridge:

```rust
pub fn mount_context(cfg: &mut web::ServiceConfig, ctx: &ModuleContext) {
    for route in ctx.routes() {
        match route.method {
            Method::Get  → cfg.route(path, web::get().to(handler_closure)),
            Method::Post → cfg.route(path, web::post().to(handler_closure)),
            // ...
        }
    }
    for scope in ctx.scopes() {
        cfg.service(web::scope(prefix).configure(|inner| mount_context(inner, &scope.context)));
    }
}
```

It iterates every `RouteDef` in the `ModuleContext` and registers it with Actix's routing table.
