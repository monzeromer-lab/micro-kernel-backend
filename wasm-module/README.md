# wasm-module

Trait contract for building WASM modules that run on micro-kernel web backends.

Zero heavy dependencies. Just implement the `WasmModule` trait, compile to
`wasm32-unknown-unknown`, and drop the `.wasm` file into the server's module
folder.

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
wasm-module = "0.1"
```

Implement the trait:

```rust
use wasm_module::{WasmModule, ModuleContext, Response};

struct MyModule;

impl WasmModule for MyModule {
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.get("/", || Response::ok("Hello from WASM!"));
        ctx.get("/health", || Response::json(b"{\"status\":\"ok\"}".to_vec()));

        ctx.scope("/admin", |admin| {
            admin.get("/dashboard", || Response::ok("Admin Dashboard"));
        });
    }
}
```

Compile:

```bash
rustc --target wasm32-unknown-unknown -O my_module.rs --crate-type cdylib
# or with cargo:
cargo build --target wasm32-unknown-unknown --release
```

Drop the resulting `.wasm` into the server's `modules/` folder. The server
loads it, calls `register()`, and mounts the routes at `/<module_name>/...`.

## What's Included

| Item | Purpose |
|------|---------|
| `WasmModule` trait | The contract — implement this |
| `ModuleContext` | Builder API for registering routes, scopes, middleware, guards |
| `Response` | Lightweight HTTP response (status, headers, body) |
| `Handler` trait | Route callback (closures work automatically) |
| `Middleware` trait | Request/response interceptor |
| `Guard` trait | Conditional routing gate |
| `ModuleProperties` | Declare memory/feature requirements |
| `Method` enum | `Get`, `Post`, `Put`, `Delete`, `Patch` |

## License

MIT OR Apache-2.0
