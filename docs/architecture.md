# Architecture

## The Micro-kernel Concept

```
┌──────────────────────────────────────────────────────────┐
│                        KERNEL                             │
│                                                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────────┐           │
│  │  Actix   │  │ wasmtime │  │   Module     │           │
│  │  HTTP    │  │  Engine  │  │   Registry   │           │
│  └──────────┘  └──────────┘  └──────────────┘           │
│                                                          │
│  ┌──────────────────────────────────────────┐           │
│  │          ServiceRegistry                  │           │
│  │  ┌────────────────┐  ┌─────────────────┐ │           │
│  │  │ Service Providers│  │ Module Exports  │ │           │
│  │  │ postgres/main_db│  │ user::get_name  │ │           │
│  │  │ http/default    │  │ order::get_info │ │           │
│  │  │ redis/cache     │  │                 │ │           │
│  │  └────────────────┘  └─────────────────┘ │           │
│  └──────────────────────────────────────────┘           │
│                                                          │
│  The kernel ONLY does:                                   │
│    • HTTP routing                                        │
│    • WASM compilation & instantiation                    │
│    • Module lifecycle (load/unload/swap)                 │
│    • Service mediation (DB, HTTP, Redis)                 │
│    • Inter-module communication                          │
└──────────────────────────────────────────────────────────┘
                          │
            ┌─────────────┼──────────────┐
            ▼             ▼              ▼
       ┌────────┐   ┌────────┐    ┌──────────┐
       │ user   │   │ order  │    │ payment  │
       │ .wasm  │◄─►│ .wasm  │    │ .wasm    │
       └───┬────┘   └────────┘    └──────────┘
           │
           ▼
    ┌─────────────┐
    │  Postgres   │   ← via kernel, never direct
    └─────────────┘
```

## Data Flow: External Service Call

```
1. Module's handler runs:
   call_service("postgres", "main_db", b"SELECT ...")
         │
2. Host callback executes (set during deploy):
   svc_registry.call_service("postgres", "main_db", ...)
         │
3. ServiceRegistry looks up "postgres/main_db"
   → finds PostgresProvider
         │
4. PostgresProvider.call(sql) runs
   (in production: uses sqlx/tokio-postgres pool)
         │
5. Returns bytes → back to module → turned into Response
```

## Data Flow: Inter-Module Call

```
Module B calls call_module("user", "get_name", args)
         │
1. Host callback executes:
   svc_registry.call_export("user", "get_name", args)
         │
2. ServiceRegistry looks up "user::get_name"
   → finds ExportEntry { module: Arc<dyn WasmModule>, function }
         │
3. Calls module.on_export_call("get_name", args)
         │
4. Returns bytes → back to Module B
```

Key insight: modules never see each other's memory. The host copies all data.
For WASM modules, the host would call into `Module A`'s wasmtime instance,
read the result from its memory, and copy it into `Module B`'s memory.

## Component Map

### Three Crates

| Crate | Purpose | Dependencies |
|-------|---------|-------------|
| `wasm-module` | The **contract** — traits and types | Zero heavy deps |
| `wasm-server` | The **kernel** — Actix + wasmtime + dashboard | actix-web, wasmtime, notify, tokio |

### Key Data Structures

#### ModuleContext (`wasm-module`)

```rust
pub struct ModuleContext {
    routes: Vec<RouteDef>,
    scopes: Vec<ScopeDef>,
    middleware: Vec<Box<dyn Middleware>>,
    guards: Vec<Box<dyn Guard>>,
    exports: Vec<String>,

    // Set by host before register() — call external services
    pub call_service: Option<Arc<dyn Fn(&str, &str, &[u8]) -> Vec<u8>>>,
    // Set by host before register() — call other modules
    pub call_module: Option<Arc<dyn Fn(&str, &str, &[u8]) -> Vec<u8>>>,
}
```

#### ServiceRegistry (`wasm-server`)

```rust
pub struct ServiceRegistry {
    services: HashMap<String, Box<dyn ServiceProvider>>,  // "postgres/main_db"
    exports: HashMap<String, ExportEntry>,                // "user::get_name"
}
```

## Trait Hierarchy (updated)

```
WasmModule: Send + Sync  ←── every module implements this
    │
    ├── register(&self, ctx: &mut ModuleContext)
    │       │
    │       ├── ctx.get/post/put/delete/patch  (routes)
    │       ├── ctx.scope                      (nesting)
    │       ├── ctx.export("name")             (inter-module exports)  ← NEW
    │       ├── ctx.middleware / ctx.guard      (interceptors)
    │       │
    │       │  Module handlers can use:
    │       ├── ctx.call_service("postgres", "main_db", sql)   ← NEW
    │       └── ctx.call_module("user", "get_name", args)      ← NEW
    │
    ├── properties(&self) → ModuleProperties
    │       memory_pages, required_services, required_modules   ← NEW
    │
    ├── version(&self) → (u16, u16, u16)
    │
    └── on_export_call(&self, function, args) → Vec<u8>        ← NEW

ServiceProvider  ←── external service wrappers
    └── call(&self, method, payload) → Vec<u8>
```

## Service Providers (built-in demos)

| Provider | Register as | What it does |
|----------|------------|-------------|
| `PostgresProvider` | `postgres/main_db` | Logs SQL, returns placeholder `{"rows":[]}` |
| `HttpClientProvider` | `http/default` | Echoes back the request body |
| `RedisProvider` | `redis/cache` | Logs command, returns `{"result":"ok"}` |

In production, these would be backed by real connection pools (`sqlx`, `reqwest`, `redis-rs`).
