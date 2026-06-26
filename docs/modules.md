# Creating WASM Modules

Every module is a Rust crate compiled to `wasm32-unknown-unknown` that implements
the `WasmModule` trait from the `wasm-module` crate.

## The Contract: `WasmModule` Trait

```rust
pub trait WasmModule {
    /// Called once when the module is loaded.
    /// Register routes, middleware, guards, and nested scopes here.
    fn register(&self, ctx: &mut ModuleContext);

    /// Declare what the module needs from the runtime.
    fn properties() -> ModuleProperties { ModuleProperties::default() }

    /// Semantic version — used by the dashboard for blue-green deployments.
    fn version() -> (u16, u16, u16) { (0, 1, 0) }
}
```

## Minimal Module

```rust
use wasm_module::{WasmModule, ModuleContext, Response};

struct HelloModule;

impl WasmModule for HelloModule {
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.get("/", || Response::ok("Hello from WASM!"));
    }
}
```

Compile to `hello.wasm`, drop in `./modules/`, and it's served at `/hello/`.

## Full Example: User Module

```rust
use std::borrow::Cow;
use wasm_module::{
    WasmModule, ModuleContext, Response, ModuleProperties,
    Middleware, Guard,
};

// ---------- Middleware ----------

struct AuthMiddleware;

impl Middleware for AuthMiddleware {
    fn name(&self) -> Cow<'static, str> {
        "auth".into()
    }

    fn before(&self) -> bool {
        // In production: read token from request headers
        // For the demo: always pass
        true
    }

    fn after(&self) -> bool {
        // In production: add response headers, log, etc.
        true
    }
}

// ---------- Guard ----------

struct AdminGuard;

impl Guard for AdminGuard {
    fn name(&self) -> Cow<'static, str> {
        "admin".into()
    }

    fn check(&self) -> bool {
        // In production: check user role
        // For the demo: deny everyone
        false
    }
}

// ---------- Module ----------

struct UserModule;

impl WasmModule for UserModule {
    fn version() -> (u16, u16, u16) {
        (1, 0, 0)
    }

    fn properties() -> ModuleProperties {
        ModuleProperties {
            memory_pages: 2,       // 128 KiB
            max_memory_pages: None, // unbounded
            ..Default::default()
        }
    }

    fn register(&self, ctx: &mut ModuleContext) {
        // Attach middleware to all routes in this scope
        ctx.middleware(AuthMiddleware);

        // Public routes
        ctx.get("/", || Response::ok("User Module — API Root"))
           .get("/list", || {
               Response::json(b"[{\"id\":1,\"name\":\"Alice\"}]".to_vec())
           })
           .post("/create", || Response::created("user created"));

        // Admin sub-scope with extra guard
        ctx.scope("/admin", |admin| {
            admin.guard(AdminGuard)
                 .get("/dashboard", || Response::ok("Admin Dashboard"))
                 .get("/stats", || Response::json(b"{\"users\":42}".to_vec()));
        });
    }
}
```

This registers:

| Method | Path | Response |
|--------|------|----------|
| GET | `/user/` | "User Module — API Root" |
| GET | `/user/list` | JSON array |
| POST | `/user/create` | 201 Created |
| GET | `/user/admin/dashboard` | "Admin Dashboard" (blocked by AdminGuard) |
| GET | `/user/admin/stats` | JSON stats (blocked by AdminGuard) |

## The ModuleContext Builder API

`ModuleContext` uses the builder pattern — every method returns `&mut Self` so you can chain:

```rust
ctx.get("/a", handler_a)
   .post("/b", handler_b)
   .put("/c", handler_c)
   .delete("/d", handler_d)
   .patch("/e", handler_e);
```

### Available Methods

| Method | Signature | Purpose |
|--------|-----------|---------|
| `get` | `(path, handler) → &mut Self` | Register a GET route |
| `post` | `(path, handler) → &mut Self` | Register a POST route |
| `put` | `(path, handler) → &mut Self` | Register a PUT route |
| `delete` | `(path, handler) → &mut Self` | Register a DELETE route |
| `patch` | `(path, handler) → &mut Self` | Register a PATCH route |
| `scope` | `(prefix, fn(&mut ModuleContext)) → &mut Self` | Create nested scope |
| `middleware` | `(impl Middleware) → &mut Self` | Attach middleware |
| `guard` | `(impl Guard) → &mut Self` | Attach guard |

### Handlers

Handlers are closures that return anything convertible to `Response`:

```rust
// Plain text
ctx.get("/hello", || "hello world");

// JSON
ctx.get("/data", || Response::json(b"{\"key\":\"val\"}".to_vec()));

// Custom status
ctx.post("/create", || Response::created("done"));
ctx.get("/missing", || Response::not_found());

// Using the Response builder
ctx.get("/custom", || Response {
    status: 418,
    headers: vec![("x-custom".into(), "value".into())],
    body: b"I'm a teapot".to_vec(),
});
```

### The Response Type

```rust
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
```

Convenience constructors:

| Constructor | Status | Content-Type |
|-------------|--------|-------------|
| `Response::ok(body)` | 200 | text/plain |
| `Response::json(body)` | 200 | application/json |
| `Response::created(body)` | 201 | text/plain |
| `Response::bad_request(body)` | 400 | text/plain |
| `Response::not_found()` | 404 | text/plain |
| `Response::internal_error(body)` | 500 | text/plain |

## Module Properties

Override `properties()` to request specific resources from the kernel:

```rust
fn properties() -> ModuleProperties {
    ModuleProperties {
        memory_pages: 4,            // 256 KiB minimum
        max_memory_pages: Some(16), // 1 MiB maximum
        memory64: false,            // 32-bit addressing
        consume_fuel: false,        // no fuel metering
        max_wasm_stack: Some(1_048_576), // 1 MiB stack
    }
}
```

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `memory_pages` | `u32` | `1` | Minimum linear memory (pages of 64 KiB) |
| `max_memory_pages` | `Option<u32>` | `None` | Maximum memory (None = unbounded) |
| `memory64` | `bool` | `false` | 64-bit memory addressing |
| `consume_fuel` | `bool` | `false` | Enable fuel-based yielding |
| `max_wasm_stack` | `Option<usize>` | `None` | Maximum WASM stack in bytes |

## Middleware in Depth

```rust
pub trait Middleware: Send + Sync + 'static {
    /// Unique name — shown in logs and dashboard.
    fn name(&self) -> Cow<'static, str>;

    /// Called BEFORE the handler. Return false to short-circuit (reject).
    fn before(&self) -> bool { true }

    /// Called AFTER the handler. Return false to signal an error.
    fn after(&self) -> bool { true }
}
```

Middleware is registered per-scope and applies to all routes within that scope
(including nested scopes). If `before()` returns `false`, the handler is skipped
and a 403 response is returned.

## Guards in Depth

```rust
pub trait Guard: Send + Sync + 'static {
    /// Unique name — shown in logs and dashboard.
    fn name(&self) -> Cow<'static, str>;

    /// Return true to allow the request, false to reject (403).
    fn check(&self) -> bool;
}
```

Guards are checked before the handler. If any guard in the chain returns `false`,
the request is rejected immediately.
