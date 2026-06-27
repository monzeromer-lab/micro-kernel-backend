# Building Modules — Complete Guide

This guide covers everything you need to know to build modules for the
Micro-kernel Architecture.  After reading this you'll be able to create
modules with routes, middleware, guards, database access, inter-module
communication, and deploy them with blue-green versioning.

---

## Table of Contents

1. [Setup](#setup)
2. [The `WasmModule` Trait](#the-wasmmodule-trait)
3. [Routes](#routes)
4. [Handlers & Responses](#handlers--responses)
5. [Nested Scopes](#nested-scopes)
6. [Middleware](#middleware)
7. [Guards](#guards)
8. [Typed Service Handles](#typed-service-handles)
9. [Inter-Module Communication](#inter-module-communication)
10. [Module Properties](#module-properties)
11. [Versioning & Blue-Green](#versioning--blue-green)
12. [Compiling to WASM](#compiling-to-wasm)
13. [Deploying](#deploying)
14. [Complete Example](#complete-example)
15. [Reference](#reference)

---

## Setup

Add the published crate to your `Cargo.toml`:

```toml
[package]
name = "my-module"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
wasm-module = "0.1"

# Optional: enable JSON deserialisation for inter-module calls
# wasm-module = { version = "0.1", features = ["json"] }
```

Create `src/lib.rs`:

```rust
use wasm_module::{WasmModule, ModuleContext, Response};

struct MyModule;

impl WasmModule for MyModule {
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.get("/", || Response::ok("Hello!"));
    }
}
```

---

## The `WasmModule` Trait

Every module implements this trait.  It has four methods — only `register()`
is mandatory.

```rust
pub trait WasmModule: Send + Sync {
    /// Called once when the module is loaded by the kernel.
    /// Register routes, middleware, guards, exports, and nested scopes here.
    fn register(&self, ctx: &mut ModuleContext);

    /// Declare what this module needs from the kernel.
    /// Override to request services, memory, features.
    fn properties(&self) -> ModuleProperties { ModuleProperties::default() }

    /// Semantic version for blue-green deployments.
    fn version(&self) -> (u16, u16, u16) { (0, 1, 0) }

    /// Called when another module invokes one of your exported functions.
    fn on_export_call(&self, function: &str, args: &[u8]) -> Vec<u8> { vec![] }
}
```

---

## Routes

Register routes using the `ModuleContext` builder.  Every method returns
`&mut Self` so you can chain calls.

### HTTP Methods

```rust
fn register(&self, ctx: &mut ModuleContext) {
    ctx.get("/users",     || Response::ok("list users"))
       .post("/users",    || Response::created("user created"))
       .put("/users/:id", || Response::ok("user updated"))
       .delete("/users/:id", || Response::ok("user deleted"))
       .patch("/users/:id", || Response::ok("user patched"));
}
```

| Method | HTTP verb | Typical use |
|--------|----------|-------------|
| `.get(path, handler)` | GET | Read |
| `.post(path, handler)` | POST | Create |
| `.put(path, handler)` | PUT | Replace |
| `.delete(path, handler)` | DELETE | Remove |
| `.patch(path, handler)` | PATCH | Partial update |

### URL Patterns

The path is relative to your module's prefix.  If your module file is
`user.wasm`, it's mounted at `/user/`.  A route registered as `"/list"`
becomes `/user/list`.

Paths can include dynamic segments and wildcards (actix-web syntax):

```rust
ctx.get("/users/{id}", || Response::ok("single user"));
ctx.get("/files/{filename:.*}", || Response::ok("any file path"));
```

---

## Handlers & Responses

### Handlers

A handler is anything that implements `Handler` — there's a blanket impl for
closures that return `impl Into<Response>`:

```rust
// Plain string → 200 OK with text/plain
ctx.get("/", || "hello world");

// JSON
ctx.get("/data", || Response::json(b"[1,2,3]".to_vec()));

// Custom status
ctx.post("/create", || Response::created("done"));

// Closure with captured state
let prefix = "User: ".to_string();
ctx.get("/greet", move || format!("{prefix}hello"));
```

### The `Response` Type

```rust
pub struct Response {
    pub status: u16,                     // HTTP status code
    pub headers: Vec<(String, String)>,   // response headers
    pub body: Vec<u8>,                    // response body bytes
}
```

**Built-in constructors:**

| Constructor | Status | Content-Type |
|-------------|--------|-------------|
| `Response::ok(body)` | 200 | `text/plain; charset=utf-8` |
| `Response::json(body)` | 200 | `application/json` |
| `Response::created(body)` | 201 | `text/plain; charset=utf-8` |
| `Response::bad_request(body)` | 400 | `text/plain; charset=utf-8` |
| `Response::not_found()` | 404 | `text/plain; charset=utf-8` |
| `Response::internal_error(body)` | 500 | `text/plain; charset=utf-8` |

**Custom response:**

```rust
ctx.get("/custom", || Response {
    status: 418,
    headers: vec![("x-teapot".into(), "yes".into())],
    body: b"I'm a teapot".to_vec(),
});
```

---

## Nested Scopes

Group routes under a URL prefix using `.scope()`:

```rust
ctx.scope("/admin", |admin| {
    admin.get("/dashboard", || Response::ok("admin dashboard"))
         .get("/users",     || Response::ok("admin user list"));

    // Scopes can nest deeper
    admin.scope("/reports", |reports| {
        reports.get("/sales", || Response::ok("sales report"));
    });
});
```

This creates:
- `GET /admin/dashboard`
- `GET /admin/users`
- `GET /admin/reports/sales`

Typed handles and callbacks are **automatically propagated** to nested scopes.
If you set `ctx.postgres` on the parent, it's available in all children.

---

## Middleware

Middleware intercepts every request in a scope.  Implement the `Middleware` trait
and register it with `.middleware()`:

```rust
use std::borrow::Cow;
use wasm_module::Middleware;

struct Logger;
impl Middleware for Logger {
    fn name(&self) -> Cow<'static, str> { "logger".into() }

    fn before(&self) -> bool {
        // Called BEFORE the handler.
        // Return false to short-circuit (reject the request).
        println!("request incoming");
        true
    }

    fn after(&self) -> bool {
        // Called AFTER the handler.
        // Return false to signal an error.
        println!("request completed");
        true
    }
}

// Register in your module
fn register(&self, ctx: &mut ModuleContext) {
    ctx.middleware(Logger)
       .get("/", || Response::ok("logged route"));
}
```

Middleware applies to all routes in the scope, including nested scopes.

---

## Guards

A guard is a boolean predicate — if it returns `false`, the request is
rejected with 403 Forbidden.  Implement the `Guard` trait and register
with `.guard()`:

```rust
use wasm_module::Guard;

struct AuthGuard;
impl Guard for AuthGuard {
    fn name(&self) -> Cow<'static, str> { "auth".into() }
    fn check(&self) -> bool {
        // In production: validate token, check headers, etc.
        // For the demo, always allow:
        true
    }
}

struct AdminGuard;
impl Guard for AdminGuard {
    fn name(&self) -> Cow<'static, str> { "admin".into() }
    fn check(&self) -> bool { false } // nobody is admin!
}

fn register(&self, ctx: &mut ModuleContext) {
    ctx.guard(AuthGuard)
       .get("/", || Response::ok("guarded!"));

    // Scope-specific guards (only admin can access)
    ctx.scope("/admin", |admin| {
        admin.guard(AdminGuard)
             .get("/dashboard", || Response::ok("admin only"));
    });
}
```

Guards are checked **before** the handler.  If any guard in the chain
returns `false`, the request is rejected immediately.

---

## Typed Service Handles

Modules access databases, caches, and HTTP APIs through **typed handles**
set by the kernel.  These are available as fields on `ModuleContext`.

### Available Handles

| Field | Type | Backend |
|-------|------|---------|
| `ctx.postgres` | `Option<Arc<dyn PostgresHandle>>` | sqlx PgPool |
| `ctx.redis` | `Option<Arc<dyn RedisHandle>>` | redis-rs |
| `ctx.mysql` | `Option<Arc<dyn MySqlHandle>>` | sqlx MySqlPool |
| `ctx.s3` | `Option<Arc<dyn S3Handle>>` | ureq (S3-compatible) |
| `ctx.http` | `Option<Arc<dyn HttpHandle>>` | ureq (HTTP client) |

### The Clone-Before-Borrow Pattern

**Important**: `ctx.get()`, `ctx.post()`, etc. take `&mut self`.  If your
handler closure captures a handle from `ctx`, you get a borrow conflict.
The fix: **clone the handle before the builder chain**, then `move` it
into the closure:

```rust
fn register(&self, ctx: &mut ModuleContext) {
    // ✅ Clone handles BEFORE mutable borrows
    let pg = ctx.postgres.clone();
    let redis = ctx.redis.clone();

    ctx.get("/list", move || {           // ← 'move' owns the clones
        let rows = pg.as_ref().unwrap()
            .query("SELECT id, name FROM users")
            .unwrap_or_default();
        Response::json(rows.into_bytes())
    })
    .get("/cache", move || {
        let val = redis.as_ref().unwrap()
            .get("homepage:stats").unwrap()
            .unwrap_or_else(|| "not cached".into());
        Response::ok(val)
    });
}
```

### Postgres Handle

```rust
pub trait PostgresHandle: Send + Sync {
    fn query(&self, sql: &str) -> Result<String, String>;
    fn execute(&self, sql: &str) -> Result<u64, String>;
    fn query_with(&self, sql: &str, params: &[&str]) -> Result<String, String>;
}
```

```rust
// SELECT → JSON rows
let rows = pg.query("SELECT id, name FROM users").unwrap();
// → {"rows":[{"id":"1","name":"Alice"}],"count":1}

// INSERT/UPDATE/DELETE → rows affected
let n = pg.execute("DELETE FROM users WHERE id = 99").unwrap();
// → 0

// Parameterised query ($1, $2, ...)
let rows = pg.query_with("SELECT * FROM users WHERE id = $1", &["42"]).unwrap();
```

### Redis Handle

```rust
pub trait RedisHandle: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str, ttl_seconds: Option<u64>) -> Result<(), String>;
    fn del(&self, keys: &[&str]) -> Result<u64, String>;
    fn incr(&self, key: &str, amount: Option<i64>) -> Result<i64, String>;
    fn exists(&self, key: &str) -> Result<bool, String>;
}
```

```rust
redis.set("counter", "0", None).unwrap();
let val: Option<String> = redis.get("counter").unwrap();  // Some("0")
let n: i64 = redis.incr("counter", None).unwrap();        // 1
let n: i64 = redis.incr("counter", Some(5)).unwrap();     // 6
let ok: bool = redis.exists("counter").unwrap();          // true
let deleted: u64 = redis.del(&["counter"]).unwrap();       // 1

// Set with TTL (expires in 60 seconds)
redis.set("session", "abc123", Some(60)).unwrap();
```

### S3 Handle

```rust
pub trait S3Handle: Send + Sync {
    fn put(&self, bucket: &str, key: &str, data: &[u8]) -> Result<String, String>;
    fn get(&self, bucket: &str, key: &str) -> Result<Vec<u8>, String>;
    fn delete(&self, bucket: &str, key: &str) -> Result<bool, String>;
    fn list(&self, bucket: &str, prefix: Option<&str>) -> Result<String, String>;
}
```

```rust
// Upload
s3.put("my-bucket", "reports/2024.csv", csv_data).unwrap();

// Download
let data: Vec<u8> = s3.get("my-bucket", "reports/2024.csv").unwrap();
let csv = String::from_utf8(data).unwrap();

// Delete
let deleted: bool = s3.delete("my-bucket", "reports/old.txt").unwrap();

// List (returns XML response)
let listing: String = s3.list("my-bucket", Some("reports/")).unwrap();
```

Works with AWS S3, MinIO, DigitalOcean Spaces, CloudFlare R2, etc.

### HTTP Handle

```rust
pub trait HttpHandle: Send + Sync {
    fn get(&self, url: &str) -> Result<String, String>;
    fn post(&self, url: &str, body: &str) -> Result<String, String>;
    fn put(&self, url: &str, body: &str) -> Result<String, String>;
    fn delete(&self, url: &str) -> Result<String, String>;
}
```

```rust
let body = http.get("https://api.github.com/zen").unwrap();
let resp = http.post("https://httpbin.org/post", r#"{"key":"val"}"#).unwrap();
```

### Service Dependencies

Declare what services your module needs in `properties()`:

```rust
fn properties(&self) -> ModuleProperties {
    ModuleProperties {
        required_services: vec![
            ServiceRequirement { kind: ServiceKind::Postgres, identifier: "main_db".into() },
            ServiceRequirement { kind: ServiceKind::Redis,   identifier: "cache".into() },
            ServiceRequirement { kind: ServiceKind::S3,      identifier: "assets".into() },
        ],
        ..Default::default()
    }
}
```

The kernel uses this to:
- Validate that services exist before loading your module
- Start providers in dependency order
- Show service dependencies in the dashboard

---

## Inter-Module Communication

Modules are isolated — they can't access each other's memory.  All
communication goes through the kernel.

### Exporting a Function (your module)

Other modules can call functions that you export:

```rust
fn register(&self, ctx: &mut ModuleContext) {
    ctx.export("get_status");   // declare function name
    ctx.export("get_user");     // can export multiple
}

fn on_export_call(&self, function: &str, args: &[u8]) -> Vec<u8> {
    match function {
        "get_status" => b"{\"status\":\"ok\"}".to_vec(),
        "get_user" => {
            // parse args, query DB, return bytes
            let id: serde_json::Value = serde_json::from_slice(args).unwrap();
            // ... do work ...
            b"{\"id\":42,\"name\":\"Alice\"}".to_vec()
        }
        _ => vec![], // unknown function → empty response
    }
}
```

### Calling Another Module

```rust
fn register(&self, ctx: &mut ModuleContext) {
    let call_mod = ctx.call_module.clone();  // clone before mutable borrows!

    ctx.get("/from-other", move || {
        // Raw call
        let bytes = call_mod.as_ref().unwrap()("user", "get_status", b"{}");
        Response::json(bytes)
    });
}
```

### Typed Inter-Module Calls (`FromModuleBytes`)

Use `call_module_typed` to parse the response directly into a Rust type:

```rust
fn register(&self, ctx: &mut ModuleContext) {
    ctx.get("/typed", move || {
        // Parse response as String
        let name: String = ctx.call_module_typed("user", "get_name", b"{}")
            .unwrap_or_default();
        Response::ok(name)
    });
}
```

**Built-in `FromModuleBytes` implementations (no extra deps):**

| Target type | How it parses |
|-------------|--------------|
| `Vec<u8>` | Identity (raw bytes) |
| `String` | UTF-8 decode |
| `i32`, `u32`, `i64`, `u64` | Parses numeric string |
| `f64` | Parses float string |
| `bool` | Parses `"true"` / `"false"` |

**With the `json` feature enabled** (`wasm-module = { features = ["json"] }`),
any `serde::Deserialize` type works:

```rust
#[derive(serde::Deserialize)]
struct UserInfo { id: u32, name: String }

let user: UserInfo = ctx
    .call_module_typed("user", "get_user", b"{\"id\":42}")
    .unwrap();
```

---

## Module Properties

The `properties()` method tells the kernel what your module needs.

```rust
pub struct ModuleProperties {
    // Memory
    pub memory_pages: u32,            // min pages (64 KiB each) — default: 1
    pub max_memory_pages: Option<u32>, // max pages (None = unbounded)
    pub memory64: bool,               // 64-bit addressing — default: false

    // Execution control
    pub consume_fuel: bool,           // fuel-based yielding — default: false
    pub max_wasm_stack: Option<usize>, // stack size in bytes — default: 512 KiB

    // Dependencies
    pub required_services: Vec<ServiceRequirement>, // DBs, caches, HTTP
    pub required_modules: Vec<String>,              // other module names
}
```

```rust
fn properties(&self) -> ModuleProperties {
    ModuleProperties {
        memory_pages: 4,
        required_services: vec![
            ServiceRequirement { kind: ServiceKind::Postgres, identifier: "main_db".into() },
            ServiceRequirement { kind: ServiceKind::Redis,   identifier: "cache".into() },
        ],
        required_modules: vec!["auth".into(), "logging".into()],
        ..Default::default()
    }
}
```

---

## Versioning & Blue-Green

Every module has a semantic version.  The kernel uses this for blue-green
deployments — each module gets two slots (blue and green), and only the
active slot serves traffic.

```rust
fn version(&self) -> (u16, u16, u16) {
    (1, 2, 0)  // major.minor.patch
}
```

When you deploy a new version:
1. The new `.wasm` is loaded into the **inactive** slot
2. If both slots are populated, the kernel **auto-swaps** — the new version
   goes live instantly
3. The previous version stays in the other slot for instant rollback

**Version flow:**
```
Deploy v1.0.0 → BLUE (empty),  GREEN v1.0.0 ● LIVE
Deploy v2.0.0 → BLUE v2.0.0 ● LIVE,  GREEN v1.0.0
Swap manually → BLUE v2.0.0,   GREEN v1.0.0 ● LIVE  (rolled back!)
```

---

## Compiling to WASM

To deploy as a `.wasm` file, compile to the `wasm32-unknown-unknown` target:

```bash
# Install the target (once)
rustup target add wasm32-unknown-unknown

# Build
cargo build --target wasm32-unknown-unknown --release
```

The output is at `target/wasm32-unknown-unknown/release/my_module.wasm`.

**Module naming rules**: lowercase a–z only.  No numbers, no underscores.
The filename stem becomes the URL prefix.

| Filename | Mounted at |
|----------|-----------|
| `user.wasm` | `/user/*` |
| `order.wasm` | `/order/*` |
| `payment.wasm` | `/payment/*` |

✅ `user.wasm` ✅ `product.wasm` ✅ `api.wasm`
❌ `User.wasm` ❌ `user_api.wasm` ❌ `user1.wasm`

---

## Deploying

### Via file drop

Copy your `.wasm` to the server's `modules/` directory:

```bash
cp target/wasm32-unknown-unknown/release/my_module.wasm ./modules/
```

The file watcher detects it automatically.

### Via dashboard

Open `http://localhost:8080/dashboard`, click **⬆ Deploy**, and select your
`.wasm` file.

### Via API

```bash
curl -F module=@my_module.wasm http://localhost:8080/api/modules/deploy
```

---

## Complete Example

A full module demonstrating routes, middleware, guards, Postgres, Redis,
S3, HTTP, inter-module exports, and blue-green versioning:

```rust
use std::borrow::Cow;
use wasm_module::{
    WasmModule, ModuleContext, Response, ModuleProperties,
    ServiceRequirement, ServiceKind,
    Middleware, Guard,
};

// ── Middleware ─────────────────────────────────────────────────────────

struct RequestLogger;
impl Middleware for RequestLogger {
    fn name(&self) -> Cow<'static, str> { "logger".into() }
    fn before(&self) -> bool {
        println!("[module] request incoming");
        true
    }
    fn after(&self) -> bool {
        println!("[module] request complete");
        true
    }
}

// ── Guard ─────────────────────────────────────────────────────────────

struct ApiKeyGuard;
impl Guard for ApiKeyGuard {
    fn name(&self) -> Cow<'static, str> { "api_key".into() }
    fn check(&self) -> bool {
        // In production: check X-Api-Key header
        true
    }
}

// ── Module ────────────────────────────────────────────────────────────

struct ProductModule;

impl WasmModule for ProductModule {
    // ── Version for blue-green deployment ──────────────────────────
    fn version(&self) -> (u16, u16, u16) { (1, 0, 0) }

    // ── Declare dependencies ───────────────────────────────────────
    fn properties(&self) -> ModuleProperties {
        ModuleProperties {
            memory_pages: 2,
            required_services: vec![
                ServiceRequirement { kind: ServiceKind::Postgres, identifier: "main_db".into() },
                ServiceRequirement { kind: ServiceKind::Redis,   identifier: "cache".into() },
                ServiceRequirement { kind: ServiceKind::S3,      identifier: "assets".into() },
            ],
            required_modules: vec!["user".into()],
            ..Default::default()
        }
    }

    // ── Register routes, middleware, guards, exports ───────────────
    fn register(&self, ctx: &mut ModuleContext) {
        // Export functions for other modules
        ctx.export("get_product_count");
        ctx.export("get_featured");

        // Clone handles BEFORE mutable borrows
        let pg = ctx.postgres.clone();
        let redis = ctx.redis.clone();
        let s3 = ctx.s3.clone();
        let http = ctx.http.clone();
        let call_mod = ctx.call_module.clone();

        // Global middleware + guard
        ctx.middleware(RequestLogger)
           .guard(ApiKeyGuard);

        // ── Public routes ──────────────────────────────────────────
        ctx.get("/", || Response::ok("Product Module API"))
           .get("/list", move || {
               let rows = pg.as_ref().unwrap()
                   .query("SELECT id, name, price FROM products")
                   .unwrap_or_else(|e| format!(r#"{{"error":"{}"}}"#, e));
               Response::json(rows.into_bytes())
           })
           .get("/featured", move || {
               let cached = redis.as_ref().unwrap()
                   .get("products:featured").unwrap();
               match cached {
                   Some(data) => Response::json(data.into_bytes()),
                   None => {
                       let rows = pg.as_ref().unwrap()
                           .query("SELECT id, name FROM products WHERE featured = true")
                           .unwrap_or_default();
                       redis.as_ref().unwrap()
                           .set("products:featured", &rows, Some(300)).ok();
                       Response::json(rows.into_bytes())
                   }
               }
           })
           .get("/images/:id", move || {
               let data = s3.as_ref().unwrap()
                   .get("products", "images/42.jpg")
                   .unwrap_or_else(|e| e.into_bytes());
               Response {
                   status: 200,
                   headers: vec![("content-type".into(), "image/jpeg".into())],
                   body: data,
               }
           })
           .get("/external-price", move || {
               let resp = http.as_ref().unwrap()
                   .get("https://api.example.com/pricing")
                   .unwrap_or_else(|e| e);
               Response::json(resp.into_bytes())
           })
           .get("/from-user", move || {
               use wasm_module::FromModuleBytes;
               let bytes = call_mod.as_ref().unwrap()("user", "get_name", b"{}");
               let name = String::from_module_bytes(&bytes).unwrap_or_default();
               Response::ok(name)
           });

        // ── Admin scope ────────────────────────────────────────────
        ctx.scope("/admin", |admin| {
            admin.get("/dashboard", || Response::ok("Admin Dashboard"))
                 .get("/stats", move || {
                     let count = pg.as_ref().unwrap()
                         .query("SELECT COUNT(*) FROM products")
                         .unwrap_or_default();
                     Response::json(count.into_bytes())
                 });
        });
    }

    // ── Handle calls from other modules ────────────────────────────
    fn on_export_call(&self, function: &str, _args: &[u8]) -> Vec<u8> {
        match function {
            "get_product_count" => b"42".to_vec(),
            "get_featured" => b"[{\"id\":1,\"name\":\"Widget\"}]".to_vec(),
            _ => vec![],
        }
    }
}
```

---

## Reference

### `WasmModule` trait methods

| Method | Required | Returns | Purpose |
|--------|----------|---------|---------|
| `register(&self, ctx)` | ✅ Yes | — | Declare routes, exports, middleware, guards |
| `properties(&self)` | No | `ModuleProperties` | Declare memory, services, dependencies |
| `version(&self)` | No | `(u16,u16,u16)` | Semantic version for blue-green |
| `on_export_call(&self, fn, args)` | No | `Vec<u8>` | Handle inter-module function calls |

### `ModuleContext` builder methods

| Method | Signature | Purpose |
|--------|-----------|---------|
| `.get(path, handler)` | `&mut Self` | Register GET route |
| `.post(path, handler)` | `&mut Self` | Register POST route |
| `.put(path, handler)` | `&mut Self` | Register PUT route |
| `.delete(path, handler)` | `&mut Self` | Register DELETE route |
| `.patch(path, handler)` | `&mut Self` | Register PATCH route |
| `.scope(prefix, fn)` | `&mut Self` | Create nested scope |
| `.export(name)` | `&mut Self` | Export function for other modules |
| `.middleware(mw)` | `&mut Self` | Attach middleware |
| `.guard(g)` | `&mut Self` | Attach guard |
| `.call_module_typed::<T>(mod, fn, args)` | `Result<T, String>` | Typed inter-module call |
| `.call_service_typed::<T>(kind, id, payload)` | `Result<T, String>` | Typed service call |

### `ModuleContext` handle fields (set by kernel)

| Field | Type | Accessible after |
|-------|------|-----------------|
| `ctx.postgres` | `Option<Arc<dyn PostgresHandle>>` | `required_services` includes Postgres |
| `ctx.redis` | `Option<Arc<dyn RedisHandle>>` | `required_services` includes Redis |
| `ctx.mysql` | `Option<Arc<dyn MySqlHandle>>` | `required_services` includes MySql |
| `ctx.s3` | `Option<Arc<dyn S3Handle>>` | `required_services` includes S3 |
| `ctx.http` | `Option<Arc<dyn HttpHandle>>` | Always available |
| `ctx.call_module` | `Option<Arc<ModuleCallFn>>` | Always available |

### `Response` constructors

| Constructor | Status |
|-------------|--------|
| `Response::ok(body)` | 200 |
| `Response::json(body)` | 200 (content-type: application/json) |
| `Response::created(body)` | 201 |
| `Response::bad_request(body)` | 400 |
| `Response::not_found()` | 404 |
| `Response::internal_error(body)` | 500 |

### `PostgresHandle` / `MySqlHandle`

| Method | Returns | Purpose |
|--------|---------|---------|
| `.query(sql)` | `Result<String>` | SELECT → JSON rows |
| `.execute(sql)` | `Result<u64>` | INSERT/UPDATE/DELETE → rows affected |
| `.query_with(sql, params)` | `Result<String>` | Parameterised query |

### `RedisHandle`

| Method | Returns | Purpose |
|--------|---------|---------|
| `.get(key)` | `Result<Option<String>>` | Get value |
| `.set(key, val, ttl)` | `Result<()>` | Set with optional TTL |
| `.del(keys)` | `Result<u64>` | Delete keys |
| `.incr(key, amount)` | `Result<i64>` | Increment |
| `.exists(key)` | `Result<bool>` | Check existence |

### `S3Handle`

| Method | Returns | Purpose |
|--------|---------|---------|
| `.put(bucket, key, data)` | `Result<String>` | Upload |
| `.get(bucket, key)` | `Result<Vec<u8>>` | Download |
| `.delete(bucket, key)` | `Result<bool>` | Delete |
| `.list(bucket, prefix)` | `Result<String>` | List (XML) |

### `HttpHandle`

| Method | Returns | Purpose |
|--------|---------|---------|
| `.get(url)` | `Result<String>` | GET → response body |
| `.post(url, body)` | `Result<String>` | POST |
| `.put(url, body)` | `Result<String>` | PUT |
| `.delete(url)` | `Result<String>` | DELETE |
