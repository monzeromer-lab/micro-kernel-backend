# Creating WASM Modules

Every module implements the `WasmModule` trait from the `wasm-module` crate.

## The Contract: `WasmModule` Trait

```rust
pub trait WasmModule: Send + Sync {
    /// Called once when loaded. Register routes, exports, middleware, guards.
    fn register(&self, ctx: &mut ModuleContext);

    /// Runtime properties (memory, required services, dependencies).
    fn properties(&self) -> ModuleProperties { ModuleProperties::default() }

    /// Semantic version for blue-green deployment.
    fn version(&self) -> (u16, u16, u16) { (0, 1, 0) }

    /// Called when another module invokes an exported function.
    fn on_export_call(&self, function: &str, args: &[u8]) -> Vec<u8> { vec![] }
}
```

## Minimal Module

```rust
use wasm_module::{WasmModule, ModuleContext, Response};

struct HelloModule;

impl WasmModule for HelloModule {
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.get("/", || Response::ok("Hello!"));
    }
}
```

Compile to `hello.wasm`, drop in `./modules/`, served at `/hello/`.

---

## Full Example: User Module

This module demonstrates **external service calls** (Postgres) and
**inter-module communication** (calling the Order module).

```rust
use std::borrow::Cow;
use wasm_module::{
    WasmModule, ModuleContext, Response, ModuleProperties,
    ServiceRequirement, ServiceKind,
    Middleware, Guard,
};

// ---------- Middleware ----------
struct AuthMiddleware;
impl Middleware for AuthMiddleware {
    fn name(&self) -> Cow<'static, str> { "auth".into() }
    fn before(&self) -> bool { /* check token */ true }
    fn after(&self) -> bool { true }
}

// ---------- Guard ----------
struct AdminGuard;
impl Guard for AdminGuard {
    fn name(&self) -> Cow<'static, str> { "admin".into() }
    fn check(&self) -> bool { /* check role */ false }
}

// ---------- Module ----------
struct UserModule;

impl WasmModule for UserModule {
    fn properties(&self) -> ModuleProperties {
        ModuleProperties {
            memory_pages: 2,
            // Declare services this module needs
            required_services: vec![
                ServiceRequirement {
                    kind: ServiceKind::Postgres,
                    identifier: "main_db".into(),
                }
            ],
            // Declare other modules this depends on
            required_modules: vec!["order".into()],
            ..Default::default()
        }
    }

    fn version(&self) -> (u16, u16, u16) { (1, 0, 0) }

    fn register(&self, ctx: &mut ModuleContext) {
        // -- Export a function for other modules to call --
        ctx.export("get_name");

        // Clone callbacks before mutable borrows
        let call_svc = ctx.call_service.clone();
        let call_mod = ctx.call_module.clone();

        // -- Middleware + Guards --
        ctx.middleware(AuthMiddleware);

        // -- Routes --
        ctx.get("/", || Response::ok("User Module Root"))
           .get("/list", move || {
               // Call the Postgres service through the kernel
               let result = if let Some(ref f) = call_svc {
                   f("postgres", "main_db", b"SELECT id, name FROM users")
               } else {
                   b"{\"error\":\"no service\"}".to_vec()
               };
               Response::json(result)
           })
           .get("/from-order", move || {
               // Call the Order module's exported function
               let result = if let Some(ref f) = call_mod {
                   f("order", "get_info", b"{}")
               } else {
                   b"{\"error\":\"no module callback\"}".to_vec()
               };
               Response::json(result)
           });

        // -- Nested scope with guard --
        ctx.scope("/admin", |admin| {
            admin.guard(AdminGuard)
                 .get("/dashboard", || Response::ok("Admin Dashboard"));
        });
    }

    fn on_export_call(&self, function: &str, _args: &[u8]) -> Vec<u8> {
        match function {
            "get_name" => b"UserModule v1.0.0".to_vec(),
            _ => vec![],
        }
    }
}
```

This registers:

| Method | Path | What happens |
|--------|------|-------------|
| GET | `/user/` | "User Module Root" |
| GET | `/user/list` | Calls Postgres → returns JSON rows |
| GET | `/user/from-order` | Calls Order module `get_info` → returns result |
| GET | `/user/admin/dashboard` | Blocked by AdminGuard |

### Cloning Callbacks Before Mutable Borrows

`ModuleContext` methods like `.get()` take `&mut self`. If your handler closure
captures `ctx.call_service` or `ctx.call_module`, you'll get a borrow conflict.
The pattern is:

```rust
fn register(&self, ctx: &mut ModuleContext) {
    let call_svc = ctx.call_service.clone();  // clone BEFORE mutable borrows
    let call_mod = ctx.call_module.clone();

    ctx.get("/path", move || {          // 'move' needed to take ownership
        if let Some(ref f) = call_svc {
            f("postgres", "main_db", b"...")
        }
        ...
    });
}
```

---

## Inter-Module Communication

### Exporting a function (Module A)

```rust
fn register(&self, ctx: &mut ModuleContext) {
    ctx.export("get_name");    // declares this function exists
    ctx.export("calculate");
}

fn on_export_call(&self, function: &str, args: &[u8]) -> Vec<u8> {
    match function {
        "get_name" => b"Module A".to_vec(),
        "calculate" => {
            // parse args (JSON or custom format), compute, return bytes
            b"42".to_vec()
        }
        _ => vec![],
    }
}
```

### Calling another module (Module B)

```rust
fn register(&self, ctx: &mut ModuleContext) {
    let call_mod = ctx.call_module.clone();

    ctx.get("/call-a", move || {
        let result = if let Some(ref f) = call_mod {
            f("module_a", "get_name", b"{}")
        } else {
            b"error".to_vec()
        };
        Response::json(result)
    });
}
```

### The host mediates everything

```
Module B ──call_module("module_a", "get_name", args)──▶ Kernel
                                                          │
                                              ServiceRegistry
                                              "module_a::get_name"
                                                          │
                                                          ▼
                                            Module A.on_export_call()
                                                          │
                         ◀────── return bytes ────────────┘
Module B ◀──────────── receives bytes
```

The kernel looks up `"module_a::get_name"` in the `ServiceRegistry`, finds the
`Arc<dyn WasmModule>` for Module A, and calls `on_export_call()` on it. The
result bytes are returned to Module B. Modules never see each other's memory.

---

## External Service Calls

```rust
fn register(&self, ctx: &mut ModuleContext) {
    let call_svc = ctx.call_service.clone();

    ctx.get("/db", move || {
        let rows = if let Some(ref f) = call_svc {
            f("postgres", "main_db", b"SELECT * FROM users")
        } else {
            b"[]".to_vec()
        };
        Response::json(rows)
    });
}
```

The kernel's `ServiceRegistry` holds registered providers:

| Kind | Identifier | Example call |
|------|-----------|-------------|
| `postgres` | `main_db` | `f("postgres", "main_db", sql_bytes)` |
| `http` | `default` | `f("http", "default", url_bytes)` |
| `redis` | `cache` | `f("redis", "cache", command_bytes)` |

### Declaring Service Dependencies

Declare what your module needs in `properties()`:

```rust
fn properties(&self) -> ModuleProperties {
    ModuleProperties {
        required_services: vec![
            ServiceRequirement { kind: ServiceKind::Postgres, identifier: "main_db".into() },
            ServiceRequirement { kind: ServiceKind::Redis, identifier: "cache".into() },
        ],
        required_modules: vec!["order".into()],
        ..Default::default()
    }
}
```

The kernel uses this to:
- Validate that required services exist before loading the module
- Start services in dependency order
- Show dependencies in the dashboard

---

## ModuleContext Builder API

| Method | Signature | Purpose |
|--------|-----------|---------|
| `get` | `(path, handler) → &mut Self` | GET route |
| `post` | `(path, handler) → &mut Self` | POST route |
| `put` | `(path, handler) → &mut Self` | PUT route |
| `delete` | `(path, handler) → &mut Self` | DELETE route |
| `patch` | `(path, handler) → &mut Self` | PATCH route |
| `scope` | `(prefix, fn(&mut ModuleContext)) → &mut Self` | Nested scope |
| `export` | `(name) → &mut Self` | Export function for other modules |
| `middleware` | `(impl Middleware) → &mut Self` | Attach middleware |
| `guard` | `(impl Guard) → &mut Self` | Attach guard |

**Handlers** are closures returning anything convertible to `Response`:

```rust
ctx.get("/hello", || "hello world");
ctx.get("/json", || Response::json(b"[1,2,3]".to_vec()));
ctx.post("/create", || Response::created("done"));
```

## Response Type

```rust
pub struct Response {
    pub status: u16,         // HTTP status code
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
```

| Constructor | Status | Content-Type |
|-------------|--------|-------------|
| `Response::ok(body)` | 200 | text/plain |
| `Response::json(body)` | 200 | application/json |
| `Response::created(body)` | 201 | text/plain |
| `Response::bad_request(body)` | 400 | text/plain |
| `Response::not_found()` | 404 | text/plain |
| `Response::internal_error(body)` | 500 | text/plain |

## ModuleProperties

```rust
pub struct ModuleProperties {
    pub memory_pages: u32,
    pub max_memory_pages: Option<u32>,
    pub memory64: bool,
    pub consume_fuel: bool,
    pub max_wasm_stack: Option<usize>,
    pub required_services: Vec<ServiceRequirement>,
    pub required_modules: Vec<String>,
}
```

## Middleware Trait

```rust
pub trait Middleware: Send + Sync + 'static {
    fn name(&self) -> Cow<'static, str>;
    fn before(&self) -> bool { true }   // false = reject
    fn after(&self) -> bool { true }    // false = error
}
```

## Guard Trait

```rust
pub trait Guard: Send + Sync + 'static {
    fn name(&self) -> Cow<'static, str>;
    fn check(&self) -> bool;            // true = allow, false = 403
}
```
