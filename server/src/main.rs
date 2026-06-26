//! Micro-kernel Architecture — Tech Talk Demo.
//!
//! ```text
//!   ┌──────────┐     ┌───────────────┐     ┌──────────────────┐
//!   │  Watcher │────▶│ ModuleRegistry │────▶│ Actix ServiceConfig│
//!   │ (notify) │     │  (blue/green)  │     │  (per-request)    │
//!   └──────────┘     └───────────────┘     └──────────────────┘
//!                          │    ▲
//!                          ▼    │
//!                   ┌──────────┴────┐
//!                   │   Dashboard   │
//!                   │  /dashboard   │
//!                   └───────────────┘
//! ```

mod dashboard;
mod engine;
mod guard;
mod middleware;
mod registry;
mod resource;
mod scope;
mod watcher;

use actix_web::{web, App, HttpServer};
use std::sync::{Arc, Mutex};

use engine::WasmtimeConfig;
use registry::ModuleRegistry;
use wasm_module::{ModuleContext, Response};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("╔══════════════════════════════════════════════╗");
    println!("║  Micro-kernel Architecture — Tech Talk Demo ║");
    println!("╠══════════════════════════════════════════════╣");
    println!("║  /wasm/              — example routes       ║");
    println!("║  /dashboard          — module dashboard     ║");
    println!("║  /api/modules        — dashboard API        ║");
    println!("║  Module folder: ./modules/                  ║");
    println!("╚══════════════════════════════════════════════╝");

    // -- Kernel ---------------------------------------------------------------
    let wasm_config = WasmtimeConfig::default().build();
    let _engine =
        wasmtime::Engine::new(&wasm_config).expect("failed to create wasmtime engine");
    println!("[kernel] wasmtime engine ready");

    // -- Shared state ---------------------------------------------------------
    let registry = Arc::new(Mutex::new(ModuleRegistry::new()));

    // -- File watcher (background) --------------------------------------------
    let watcher_registry = Arc::clone(&registry);
    tokio::spawn(async move {
        println!("[watcher] watching ./modules/ for .wasm files...");
        match watcher::ModuleWatcher::start("./modules") {
            Ok(watcher) => {
                for event in watcher.rx.iter() {
                    match event {
                        watcher::WatchEvent::Added(name) => {
                            println!("[watcher] module added: {name}");
                            // TODO: auto-deploy on file drop (demo placeholder)
                        }
                        watcher::WatchEvent::Modified(name) => {
                            println!("[watcher] module modified: {name}");
                            // TODO: auto-redeploy into inactive slot (blue-green)
                        }
                        watcher::WatchEvent::Removed(name) => {
                            println!("[watcher] module removed: {name}");
                            if let Ok(mut reg) = watcher_registry.lock() {
                                reg.remove(&name);
                            }
                        }
                    }
                }
            }
            Err(e) => eprintln!("[watcher] failed: {e}"),
        }
    });

    // -- Example scope (demo of the module-dev DX) ----------------------------
    let mut example_ctx = ModuleContext::new();
    example_ctx
        .get("/", || Response::ok("Hello from the dynamic scope!"))
        .get("/health", || {
            Response::json(r#"{"status":"ok"}"#.as_bytes().to_vec())
        });

    let example_ctx = Arc::new(example_ctx);

    // -- Actix HTTP server ----------------------------------------------------
    HttpServer::new(move || {
        let reg = Arc::clone(&registry);
        let example_ctx = Arc::clone(&example_ctx);

        App::new()
            // App data
            .app_data(web::Data::new(Arc::clone(&reg)))
            // Dashboard API + UI
            .configure(dashboard::configure)
            // Example scope
            .configure(move |cfg| {
                cfg.service(
                    actix_web::web::scope("/wasm").configure(|inner| {
                        scope::mount_context(inner, &example_ctx);
                    }),
                );
            })
            // WASM modules from the registry
            .configure(move |cfg| {
                if let Ok(reg) = reg.lock() {
                    reg.configure_all(cfg);
                }
            })
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
