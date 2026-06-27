# Project Summary — Micro-kernel Architecture

This document is a **self-contained, deep-dive summary** of the entire project.
Share it with your team so everyone understands the concept, the architecture,
and how everything fits together.

---

## Table of Contents

1. [What Is This?](#what-is-this)
2. [The Problem It Solves](#the-problem-it-solves)
3. [High-Level Architecture](#high-level-architecture)
4. [The Two Crates](#the-two-crates)
5. [How a Module Is Written](#how-a-module-is-written)
6. [How a Module Is Loaded](#how-a-module-is-loaded)
7. [How a Request Is Served](#how-a-request-is-served)
8. [Blue-Green Deployment](#blue-green-deployment)
9. [Inter-Module Communication](#inter-module-communication)
10. [External Services](#external-services)
11. [Middleware & Guards](#middleware--guards)
12. [The Dashboard](#the-dashboard)
13. [Project Structure](#project-structure)
14. [Key Design Decisions](#key-design-decisions)
15. [What This Isn't (Yet)](#what-this-isnt-yet)

---

## What Is This?

A **micro-kernel web backend** — a server where the core (the "kernel") does
only the bare minimum and all business logic lives in **dynamically-loaded
WebAssembly modules**.

Think of it like a micro-kernel operating system (Minix, QNX, L4) applied to a
web server:

| OS Concept | Web Equivalent |
|-----------|---------------|
| Kernel | Actix HTTP server + wasmtime runtime + service registry |
| User-space process | A `.wasm` module (compiled from Rust) |
| IPC | Host function calls (kernel mediates all communication) |
| Process lifecycle | Blue-green deploy, swap, rollback |
| Device drivers | Service providers (Postgres, HTTP, Redis) |

The kernel **never** contains business logic. It only routes HTTP requests,
manages WASM instances, and brokers communication between modules and external
services. Every feature — users, orders, payments, dashboards — is a module.

---

## The Problem It Solves

In a traditional monolithic backend:

```
┌─────────────────────────────────────────┐
│  Single binary                           │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌────────┐  │
│  │Users │ │Orders│ │Payment│ │Analytics│  │
│  └──────┘ └──────┘ └──────┘ └────────┘  │
│                                          │
│  To deploy "Users v2":                   │
│    1. Rebuild entire binary              │
│    2. Run full test suite                │
│    3. Restart the whole server           │
│    4. Pray nothing broke                 │
└─────────────────────────────────────────┘
```

With micro-kernel architecture:

```
┌──────────────────────────────────────────┐
│  Kernel (100 KB, never changes)           │
│  ┌──────────┐ ┌──────────┐              │
│  │  Actix   │ │ wasmtime │              │
│  └──────────┘ └──────────┘              │
│                                          │
│  Modules (independent .wasm files)        │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌────────┐  │
│  │Users │ │Orders│ │Payment│ │Analytics│  │
│  │ v1.0 │ │ v2.1 │ │ v1.3 │ │ v0.9  │  │
│  └──────┘ └──────┘ └──────┘ └────────┘  │
│                                          │
│  To deploy "Users v2":                   │
│    1. Drop users_v2.wasm in modules/     │
│    2. Kernel loads it into green slot    │
│    3. Click "Swap" in dashboard          │
│    4. v1 stays in blue slot (rollback)   │
│    5. Zero other modules affected        │
└──────────────────────────────────────────┘
```

**Key properties:**
- Deploy one module without touching the rest
- Instant rollback (previous version is still in memory)
- Modules are isolated — a crash in `payment.wasm` doesn't take down `users.wasm`
- The kernel is tiny and rarely changes

---

## High-Level Architecture

```
                        ┌──────────────────────┐
                        │     HTTP Request     │
                        └──────────┬───────────┘
                                   │
                                   ▼
┌──────────────────────────────────────────────────────────────┐
│                         KERNEL                                │
│                                                               │
│  ┌─────────────┐   ┌─────────────┐   ┌────────────────────┐  │
│  │ Actix Router │──▶│   Module    │──▶│  Service Registry  │  │
│  │ (matches URL)│   │  Registry   │   │                    │  │
│  └─────────────┘   │             │   │ ┌────────────────┐ │  │
│                    │ user → blue │   │ │ Service        │ │  │
│                    │ order→ green│   │ │ Providers      │ │  │
│                    └──────┬──────┘   │ │ postgres/main  │ │  │
│                           │          │ │ http/default   │ │  │
│                           ▼          │ │ redis/cache    │ │  │
│                    ┌─────────────┐   │ └────────────────┘ │  │
│                    │  wasmtime   │   │                     │  │
│                    │  Engine     │   │ ┌────────────────┐ │  │
│                    └─────────────┘   │ │ Module Exports │ │  │
│                                      │ │ user::get_name │ │  │
│                                      │ │ order::get_info│ │  │
│                                      │ └────────────────┘ │  │
│                                      └────────────────────┘  │
│                                                               │
│  ┌──────────┐   ┌──────────┐   ┌──────────────────────────┐  │
│  │ Watcher  │   │Dashboard │   │  Shutdown Handle         │  │
│  │ (notify) │   │  /api/*  │   │  (graceful / force)      │  │
│  └──────────┘   └──────────┘   └──────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
                                   │
                    ┌──────────────┼──────────────┐
                    ▼              ▼              ▼
              ┌─────────┐  ┌─────────┐  ┌───────────┐
              │  user.  │  │ order.  │  │ payment.  │
              │  wasm   │  │ wasm    │  │ wasm      │
              └─────────┘  └─────────┘  └───────────┘
```

---

## The Two Crates

### `wasm-module` — The SDK (what module authors use)

```
wasm-module/
├── Cargo.toml      ← name = "wasm-module", zero heavy deps
└── src/lib.rs      ← WasmModule trait, ModuleContext, Response, etc.
```

**Public API surface:**

| Type | Kind | Purpose |
|------|------|---------|
| `WasmModule` | Trait | The contract every module implements |
| `ModuleContext` | Struct | Builder API for registering routes, exports, middleware, guards |
| `Response` | Struct | Lightweight HTTP response (status, headers, body) |
| `ModuleProperties` | Struct | Memory, services, dependencies a module requires |
| `Handler` | Trait | Route callback (blanket impl for closures) |
| `Middleware` | Trait | Request/response interceptor |
| `Guard` | Trait | Conditional routing gate |
| `ServiceRequirement` | Struct | Declares an external service dependency |
| `ServiceKind` | Enum | `Postgres`, `Http`, `Redis` |
| `Method` | Enum | `Get`, `Post`, `Put`, `Delete`, `Patch` |
| `RouteDef` | Struct | A registered route (method + path + handler) |
| `ScopeDef` | Struct | A nested scope (prefix + sub-context) |

This crate is **publishable to crates.io**. A module author adds:

```toml
[dependencies]
wasm-module = "0.1"
```

### `wasm-server` — The Kernel (the runtime)

```
server/
├── Cargo.toml      ← depends on actix-web, wasmtime, notify, tokio, wasm-module
├── static/
│   └── dashboard.html
└── src/
    ├── main.rs         ← Startup, deploys demo modules, starts server
    ├── dashboard.rs    ← Dashboard API + shutdown endpoints
    ├── scope.rs        ← ModuleContext → Actix ServiceConfig bridge
    ├── registry.rs     ← ModuleRegistry with blue-green slots
    ├── services.rs     ← ServiceRegistry (providers + exports)
    ├── resource.rs     ← Resource wrapper
    ├── middleware.rs    ← Re-exports wasm_module::Middleware
    ├── guard.rs        ← Re-exports wasm_module::Guard
    ├── watcher.rs      ← notify file watcher
    └── engine/
        ├── mod.rs
        ├── wasm_config.rs  ← wasmtime Config builder
        └── host_funcs.rs   ← Host functions for WASM modules
```

---

## How a Module Is Written

Every module is a Rust struct that implements `WasmModule`:

```rust
use wasm_module::{WasmModule, ModuleContext, Response, ModuleProperties,
                  ServiceRequirement, ServiceKind, Middleware, Guard};

struct UserModule;

impl WasmModule for UserModule {
    // 1. Declare what this module needs from the kernel
    fn properties(&self) -> ModuleProperties {
        ModuleProperties {
            memory_pages: 2,
            required_services: vec![
                ServiceRequirement { kind: ServiceKind::Postgres, identifier: "main_db".into() }
            ],
            required_modules: vec!["order".into()],
            ..Default::default()
        }
    }

    // 2. Version for blue-green deployment
    fn version(&self) -> (u16, u16, u16) { (1, 0, 0) }

    // 3. Register routes, exports, middleware, guards
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.export("get_name");  // other modules can call this

        // Clone typed handles before mutable borrows
        let pg = ctx.postgres.clone();
        let redis = ctx.redis.clone();
        let call_mod = ctx.call_module.clone();

        ctx.middleware(AuthMiddleware)
           .guard(AdminGuard)
           .get("/", || Response::ok("User Module"))
           .get("/list", move || {
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
           .get("/from-order", move || {
               use wasm_module::FromModuleBytes;
               let bytes = call_mod.as_ref().unwrap()("order", "get_info", b"{}");
               let info: String = String::from_module_bytes(&bytes).unwrap_or_default();
               Response::json(info.into_bytes())
           });
    }

    // 4. Handle calls from other modules
    fn on_export_call(&self, function: &str, _args: &[u8]) -> Vec<u8> {
        match function {
            "get_name" => b"UserModule v1.0.0".to_vec(),
            _ => vec![],
        }
    }
}
```

**The module author never touches:**
- Actix-web APIs
- wasmtime APIs
- HTTP routing internals
- Other modules' code

They only implement a trait and use a builder API (`ModuleContext`).

---

## How a Module Is Loaded

### Native Path (current demo)

```
1. Kernel startup
   │
2. Create ModuleContext with callbacks and typed handles wired to ServiceRegistry
   │   ctx.call_service = Arc::new(|kind, id, payload| { svc_registry.call_service(...) })
   │   ctx.call_module  = Arc::new(|mod, func, args|  { svc_registry.call_export(...)  })
   │   ctx.postgres     = Arc::new(ServiceRegistryPostgresHandle(svc))
   │   ctx.redis        = Arc::new(ServiceRegistryRedisHandle(svc))
   │   ctx.s3           = Arc::new(ServiceRegistryS3Handle(svc))
   │   ctx.http         = Arc::new(ServiceRegistryHttpHandle(svc))
   │
3. Create module instance
   │   let module = Arc::new(UserModule);
   │
4. Call module.register(&mut ctx)
   │   The module populates ctx with route definitions, exports, etc.
   │
5. Register exports in ServiceRegistry
   │   svc_registry.register_exports("user", &ctx, module.clone());
   │
6. Deploy into ModuleRegistry (blue-green slots)
   │   registry.deploy("user", ctx, (1,0,0), Some(module));
   │
7. Actix builds routing table
   │   registry.configure_all(cfg)
   │       → web::scope("/user").configure(|inner| mount_context(inner, ctx))
   │           → for each route: cfg.route(path, web::get().to(handler))
   │
8. Module is live at /user/*
```

### WASM Path (future)

```
1. File watcher detects user.wasm in modules/
2. wasmtime compiles the .wasm bytes → Module
3. Create Store<HostState>, Linker with host functions
4. Instantiate WASM module, call exported init()
5. Module calls host functions like register_route("GET", "/list")
6. Host builds ModuleContext from these calls
7. Deploy into ModuleRegistry (same as native path from step 6)
```

---

## How a Request Is Served

```
1. HTTP GET /user/list arrives
         │
2. Actix matches route (registered during step 7 above)
         │
3. Handler closure executes:
   │   pg.query("SELECT ...")   ← typed Postgres handle
   │       │
   │       ▼
   │   ServiceRegistryPostgresHandle.query() delegates to ServiceRegistry
   │       │
   │       ▼
   │   PostgresProvider.call() → real sqlx pool execution
   │       │
   │       ▼
   │   Returns bytes: {"rows":[{"id":1,"name":"Alice"}]}
   │
4. Handler wraps in Response::json(bytes)
         │
5. scope.rs converts rayna_module::Response → actix_web::HttpResponse
         │
6. HTTP response sent to client
```

For inter-module calls (e.g., `/user/from-order` calling `order::get_info`):

```
3. Handler closure executes:
   │   call_module("order", "get_info", b"{}")
   │       │
   │       ▼
   │   ServiceRegistry.call_export("order", "get_info", ...)
   │       │
   │       ▼
   │   OrderModule.on_export_call("get_info", b"{}")
   │       │
   │       ▼
   │   Returns bytes: "OrderModule v1.0.0 -- 42 orders pending"
   │
4. Handler wraps in Response::json(bytes)
```

---

## Blue-Green Deployment

Each module has two slots — `blue` and `green`. One is **active** (serves traffic),
the other is **standby** (idle, ready to swap).

```rust
struct ModuleSlots {
    active: String,               // "blue" or "green"
    blue: Option<ModuleEntry>,     // one version
    green: Option<ModuleEntry>,    // another version
}

struct ModuleEntry {
    version: (u16, u16, u16),
    ctx: Arc<ModuleContext>,       // route definitions
    deployed_at: String,
    module: Option<Arc<dyn WasmModule>>, // for inter-module exports
}
```

### Lifecycle

```
Initial:   (empty)

Deploy v1:
  BLUE  (empty)       GREEN v1.0.0  ● LIVE

Deploy v2:
  BLUE  v2.0.0  ● LIVE   GREEN v1.0.0       ← auto-swapped

Deploy v3:
  BLUE  v2.0.0           GREEN v3.0.0  ● LIVE   ← auto-swapped (v1 overwritten)

Manual Swap (rollback):
  BLUE  v2.0.0  ● LIVE   GREEN v3.0.0           ← one field assignment
```

The swap is `slots.active = "green"` — a single field write. No copying, no
recompilation, no downtime. The dashboard and API both expose Swap.

### Why Two Slots, Not N?

Two slots is the simplest useful model:
- One live, one standby — instant rollback
- No complexity of canary percentages or traffic splitting
- Fits the "micro-kernel" philosophy of minimal kernel, maximum simplicity

For full version history, you'd store a log separately. The registry only keeps
the current and previous version.

---

## Inter-Module Communication

Modules are **isolated** — they cannot access each other's memory or call each
other directly. All communication goes through the kernel.

### Exporting a Function (Module A)

```rust
fn register(&self, ctx: &mut ModuleContext) {
    ctx.export("get_name");   // declares this function exists
}

fn on_export_call(&self, function: &str, args: &[u8]) -> Vec<u8> {
    match function {
        "get_name" => b"Module A".to_vec(),
        "calculate" => {
            let input: MyInput = parse_json(args);
            let output = do_work(input);
            serialize_json(&output)
        }
        _ => vec![],
    }
}
```

### Calling a Module (Module B)

```rust
fn register(&self, ctx: &mut ModuleContext) {
    let call_mod = ctx.call_module.clone();

    ctx.get("/call-a", move || {
        let result = call_mod.as_ref().unwrap()("module_a", "get_name", b"{}");
        Response::json(result)
    });
}
```

### What the Kernel Does

```
Module B                      Kernel                       Module A
   │                            │                            │
   │ call_module("module_a",    │                            │
   │   "get_name", args)        │                            │
   │ ─────────────────────────▶ │                            │
   │                            │ lookup "module_a::get_name" │
   │                            │ in ServiceRegistry          │
   │                            │                            │
   │                            │ on_export_call("get_name")─▶│
   │                            │                            │
   │                            │ ◀──── return bytes ─────────│
   │                            │                            │
   │ ◀── return bytes ──────────│                            │
```

The data format is raw bytes — modules choose their own serialisation (JSON,
MessagePack, Protobuf, etc.). The kernel doesn't inspect or transform the data.

---

## External Services

Modules never open sockets or connect to databases directly. They use **typed
handles** provided by the kernel — `pg.query()`, `redis.get()`, `s3.put()`.

### Calling a Service

```rust
// In a module handler:
let rows = pg.as_ref().unwrap()
    .query("SELECT id, name FROM users WHERE active = true")
    .unwrap_or_else(|e| format!(r#"{{"error":"{}"}}"#, e));
```

### Registering a Provider

```rust
// At kernel startup:
let pg = PostgresProvider::connect(PostgresConfig {
    url: std::env::var("DATABASE_URL").unwrap(),
    max_connections: 20,
    ..Default::default()
}).await?;
service_registry.register_service("postgres", "main_db", pg);
```

### Built-in Providers

| Provider | Backend | Typed trait |
|----------|---------|------------|
| `PostgresProvider` | `sqlx::PgPool` | `PostgresHandle` |
| `MySqlProvider` | `sqlx::MySqlPool` | `MySqlHandle` |
| `RedisProvider` | `redis::Connection` | `RedisHandle` |
| `S3Provider` | `ureq::Agent` | `S3Handle` |
| `HttpProvider` | `ureq::Agent` | `HttpHandle` |

If the real backend is unavailable, an `EchoProvider` fallback is registered
so the demo still runs.

### Adding Real Providers

Implement the `ServiceProvider` trait:

```rust
pub trait ServiceProvider: Send + Sync {
    fn call(&self, method: &str, payload: &[u8]) -> Vec<u8>;
}
```

For async services (Postgres, HTTP, Redis), use `tokio::runtime::Handle::current().block_on(...)`
inside `call()` to bridge sync→async.

See `docs/services.md` for full examples with sqlx, reqwest, and redis-rs.

---

## Middleware & Guards

### Middleware

```rust
pub trait Middleware: Send + Sync + 'static {
    fn name(&self) -> Cow<'static, str>;

    /// Called BEFORE the handler. Return false to short-circuit (reject).
    fn before(&self) -> bool { true }

    /// Called AFTER the handler. Return false for error.
    fn after(&self) -> bool { true }
}
```

Registered per-scope: `ctx.middleware(AuthMiddleware)`.
Applies to all routes in that scope and nested scopes.

### Guards

```rust
pub trait Guard: Send + Sync + 'static {
    fn name(&self) -> Cow<'static, str>;

    /// Return true to allow, false to reject with 403.
    fn check(&self) -> bool;
}
```

Registered per-scope: `ctx.guard(AdminGuard)`.

---

## The Dashboard

```
http://localhost:8080/dashboard
```

| Feature | Where |
|---------|-------|
| View all modules | Module cards with blue/green slots |
| Deploy | Upload `.wasm` file |
| Swap | Blue ↔ green (instant rollback) |
| Remove | Delete module |
| Graceful Shutdown | Stop accepting, drain requests, exit |
| Force Shutdown | Kill immediately |
| Auto-refresh | Every 6 seconds |

### Dashboard API

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/modules` | List all modules |
| `POST` | `/api/modules/deploy` | Upload `.wasm` |
| `POST` | `/api/modules/{name}/swap` | Swap blue ↔ green |
| `DELETE` | `/api/modules/{name}` | Remove module |
| `POST` | `/api/shutdown/graceful` | Graceful shutdown |
| `POST` | `/api/shutdown/force` | Force shutdown |

---

## Project Structure

```
wasm/
├── Cargo.toml              ← Workspace root (2 members)
├── README.md
├── docs/                   ← 7 documentation files
│   ├── README.md           ← Overview + quick start
│   ├── SUMMARY.md          ← ← YOU ARE READING THIS
│   ├── architecture.md     ← Internal data flow, trait hierarchy
│   ├── modules.md          ← Module authoring guide
│   ├── services.md         ← Adding service providers
│   ├── dashboard.md        ← Dashboard UI guide
│   ├── api.md              ← REST API reference
│   └── blue-green.md       ← Deployment mechanism deep dive
├── modules/                ← Drop .wasm files here
│
├── wasm-module/            ← Module SDK crate (publishable to crates.io)
│   ├── Cargo.toml
│   ├── README.md
│   └── src/lib.rs          ← 400 lines — all public API
│
└── server/                 ← Kernel runtime
    ├── Cargo.toml           ← actix-web, wasmtime, notify, tokio
    ├── static/dashboard.html
    └── src/
        ├── main.rs           ← 240 lines — startup, demo modules, server
        ├── dashboard.rs      ← 140 lines — API + shutdown handlers
        ├── scope.rs          ← 75 lines — ModuleContext → Actix bridge
        ├── registry.rs       ← 195 lines — ModuleRegistry + blue-green
        ├── services.rs       ← 146 lines — ServiceRegistry + providers
        ├── resource.rs       ← 23 lines — Resource wrapper
        ├── middleware.rs     ←  3 lines — re-export
        ├── guard.rs          ←  3 lines — re-export
        ├── watcher.rs        ← 100 lines — File watcher + name validation
        └── engine/
            ├── mod.rs         ←  5 lines — re-exports
            ├── wasm_config.rs ← 90 lines — wasmtime Config builder
            └── host_funcs.rs  ← 90 lines — Host functions for WASM modules
```

**Total server code: ~1,170 lines of Rust.** The kernel is genuinely small.

---

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Module isolation | WASM sandbox | Memory safety, no shared state between modules |
| Communication model | Host-mediated | Modules never see each other's memory. Kernel copies data. |
| Data format | Raw bytes (`Vec<u8>`) | Universal. Modules pick their own serialisation (JSON, Protobuf, etc.) |
| Handler storage | Closures in `Box<dyn Handler>` | Zero-cost when native, replaced by wasmtime `Func` for WASM |
| Service access | Typed handle traits (`PostgresHandle`, etc.) + `ServiceProvider` fallback | Ergonomic, type-safe API per service. One interface for all backends. |
| Blue-green slots | Exactly two per module | Simplest useful model. Instant rollback without complexity. |
| SDK as separate crate | `wasm-module` (zero heavy deps) | Module authors don't pull in actix-web or wasmtime. Publishable to crates.io. |
| Trait-based module contract | `WasmModule` trait | Type-safe. Compiler checks that every module implements the required methods. |
| Module context as builder | `&mut self` chainable methods | Ergonomic. `ctx.get("/a", h1).post("/b", h2).scope("/x", ...)` |
| Dashboard as server endpoint | In-process HTML + API | No separate frontend to deploy. One binary, one port. |

---

## What This Isn't (Yet)

This is a **tech talk demo** that demonstrates the architecture with working code.
All 56 tests pass. Providers connect to real databases when env vars are set,
and fall back to echo implementations when they're not.

| Limitation | Current state | Production path |
|-----------|--------------|----------------|
| WASM modules | Trait implemented by native Rust structs + WASM test module | Compile all modules to `wasm32-unknown-unknown` |
| Database connections | Real `sqlx` pools with OS-thread async execution | Already production-ready with env vars |
| HTTP/S3 client | Real `ureq` sync client | Already production-ready |
| Redis | Real `redis-rs` with Mutex-wrapped connection | Pooled connections (r2d2) |
| Persistence | Everything in-memory | Store registry state on disk |
| Security | None (same-process demo) | WASM sandbox + resource limits per module |
| Version history | Two slots (blue/green) | Separate deployment log |
| Canary deployments | All-or-nothing per module | Traffic splitting percentage |
