//! Micro-kernel Architecture — Tech Talk Demo.

mod dashboard;
mod engine;
mod guard;
mod middleware;
mod registry;
mod resource;
mod scope;
mod services;
mod watcher;

use actix_web::{web, App, HttpServer};
use std::sync::{Arc, Mutex};

use actix_web::dev::ServerHandle;
use engine::WasmtimeConfig;
use registry::ModuleRegistry;
use services::ServiceRegistry;
use wasm_module::{ModuleContext, ModuleProperties, Response, ServiceKind, WasmModule};

pub type ShutdownHandle = Arc<Mutex<Option<ServerHandle>>>;

// ---------------------------------------------------------------------------
// Example modules
// ---------------------------------------------------------------------------

struct UserModule;

impl WasmModule for UserModule {
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.export("get_name");

        // Clone the callbacks BEFORE the mutable borrows
        let call_svc = ctx.call_service.clone();
        let call_mod = ctx.call_module.clone();

        ctx.get("/", || Response::ok("User Module — /user/"))
           .get("/list", move || {
               let result = if let Some(ref f) = call_svc {
                   f("postgres", "main_db", b"SELECT id, name FROM users")
               } else {
                   b"{\"error\":\"no service\"}".to_vec()
               };
               Response::json(result)
           })
           .get("/from-order", move || {
               let result = if let Some(ref f) = call_mod {
                   f("order", "get_info", b"{}")
               } else {
                   b"{\"error\":\"no module callback\"}".to_vec()
               };
               Response::json(result)
           });
    }

    fn properties(&self) -> ModuleProperties {
        ModuleProperties {
            memory_pages: 2,
            required_services: vec![wasm_module::ServiceRequirement {
                kind: ServiceKind::Postgres,
                identifier: "main_db".into(),
            }],
            required_modules: vec!["order".into()],
            ..Default::default()
        }
    }

    fn version(&self) -> (u16, u16, u16) { (1, 0, 0) }

    fn on_export_call(&self, function: &str, _args: &[u8]) -> Vec<u8> {
        match function {
            "get_name" => b"UserModule v1.0.0".to_vec(),
            _ => vec![],
        }
    }
}

// ---------------------------------------------------------------------------

struct OrderModule;

impl WasmModule for OrderModule {
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.export("get_info");

        let call_mod = ctx.call_module.clone();

        ctx.get("/", || Response::ok("Order Module — /order/"))
           .get("/call-user", move || {
               let result = if let Some(ref f) = call_mod {
                   f("user", "get_name", b"{}")
               } else {
                   b"{\"error\":\"no module callback\"}".to_vec()
               };
               Response::json(result)
           });
    }

    fn version(&self) -> (u16, u16, u16) { (1, 0, 0) }

    fn on_export_call(&self, function: &str, _args: &[u8]) -> Vec<u8> {
        match function {
            "get_info" => b"OrderModule v1.0.0 -- 42 orders pending".to_vec(),
            _ => vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("╔══════════════════════════════════════════════╗");
    println!("║  Micro-kernel Architecture — Tech Talk Demo ║");
    println!("╠══════════════════════════════════════════════╣");
    println!("║  /wasm/              — example routes       ║");
    println!("║  /user/*             — user module          ║");
    println!("║  /order/*            — order module         ║");
    println!("║  /dashboard          — module dashboard     ║");
    println!("║  /api/...            — dashboard API        ║");
    println!("╚══════════════════════════════════════════════╝");

    // -- Kernel ---------------------------------------------------------------
    let wasm_config = WasmtimeConfig::default().build();
    let _engine = wasmtime::Engine::new(&wasm_config).expect("failed to create wasmtime engine");
    println!("[kernel] wasmtime engine ready");

    // -- Shared state ---------------------------------------------------------
    let registry = Arc::new(Mutex::new(ModuleRegistry::new()));
    let shutdown_handle: ShutdownHandle = Arc::new(Mutex::new(None));

    // -- Service Registry -----------------------------------------------------
    let mut service_registry = ServiceRegistry::new();
    service_registry.register_service("postgres", "main_db", services::PostgresProvider { label: "main".into() });
    service_registry.register_service("http", "default", services::HttpClientProvider);
    service_registry.register_service("redis", "cache", services::RedisProvider { label: "cache".into() });
    println!("[services] registered: postgres/main_db, http/default, redis/cache");

    let service_registry = Arc::new(Mutex::new(service_registry));

    // -- Watcher --------------------------------------------------------------
    let watcher_registry = Arc::clone(&registry);
    tokio::spawn(async move {
        println!("[watcher] watching ./modules/ for .wasm files...");
        match watcher::ModuleWatcher::start("./modules") {
            Ok(watcher) => {
                for event in watcher.rx.iter() {
                    match event {
                        watcher::WatchEvent::Added(name) => println!("[watcher] module added: {name}"),
                        watcher::WatchEvent::Modified(name) => println!("[watcher] module modified: {name}"),
                        watcher::WatchEvent::Removed(name) => {
                            println!("[watcher] module removed: {name}");
                            if let Ok(mut reg) = watcher_registry.lock() { reg.remove(&name); }
                        }
                    }
                }
            }
            Err(e) => eprintln!("[watcher] failed: {e}"),
        }
    });

    // -- Deploy example modules -----------------------------------------------
    {
        let mut reg = registry.lock().unwrap();
        let mut svc = service_registry.lock().unwrap();

        // User module
        let user_mod: Arc<dyn WasmModule> = Arc::new(UserModule);
        let mut user_ctx = build_module_context(Arc::clone(&service_registry));
        user_mod.register(&mut user_ctx);
        svc.register_exports("user", &user_ctx, Arc::clone(&user_mod));
        reg.deploy("user", user_ctx, (1, 0, 0), Some(user_mod));
        println!("[deploy] user module v1.0.0 → /user/*");

        // Order module
        let order_mod: Arc<dyn WasmModule> = Arc::new(OrderModule);
        let mut order_ctx = build_module_context(Arc::clone(&service_registry));
        order_mod.register(&mut order_ctx);
        svc.register_exports("order", &order_ctx, Arc::clone(&order_mod));
        reg.deploy("order", order_ctx, (1, 0, 0), Some(order_mod));
        println!("[deploy] order module v1.0.0 → /order/*");
    }

    // -- Example scope --------------------------------------------------------
    let mut example_ctx = ModuleContext::new();
    example_ctx
        .get("/", || Response::ok("Hello from the dynamic scope!"))
        .get("/health", || Response::json(r#"{"status":"ok"}"#.as_bytes().to_vec()));
    let example_ctx = Arc::new(example_ctx);

    // -- Build the server -----------------------------------------------------
    let srv_handle = Arc::clone(&shutdown_handle);
    let server = HttpServer::new(move || {
        let reg = Arc::clone(&registry);
        let example_ctx = Arc::clone(&example_ctx);
        let shutdown = Arc::clone(&srv_handle);

        App::new()
            .app_data(web::Data::new(Arc::clone(&reg)))
            .app_data(web::Data::new(shutdown))
            .configure(dashboard::configure)
            .configure(move |cfg| {
                cfg.service(actix_web::web::scope("/wasm")
                    .configure(|inner| scope::mount_context(inner, &example_ctx)));
            })
            .configure(move |cfg| {
                if let Ok(reg) = reg.lock() { reg.configure_all(cfg); }
            })
    })
    .bind("127.0.0.1:8080")?
    .run();

    *shutdown_handle.lock().unwrap() = Some(server.handle());
    println!("[kernel] server listening on http://127.0.0.1:8080");
    server.await
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_module_context(svc: Arc<Mutex<ServiceRegistry>>) -> ModuleContext {
    let mut ctx = ModuleContext::new();
    let svc1 = Arc::clone(&svc);
    let svc2 = Arc::clone(&svc);

    ctx.call_service = Some(Arc::new(move |kind: &str, id: &str, payload: &[u8]| {
        svc1.lock().unwrap().call_service(kind, id, "", payload)
    }));

    ctx.call_module = Some(Arc::new(move |module: &str, func: &str, args: &[u8]| {
        svc2.lock().unwrap().call_export(module, func, args)
    }));

    ctx
}
