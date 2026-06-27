//! Micro-kernel Architecture — Tech Talk Demo.

mod dashboard;
mod engine;
mod guard;
mod middleware;
mod providers;
mod registry;
mod resource;
mod scope;
mod services;
mod watcher;

use actix_web::{web, App, HttpServer};
use actix_web::dev::ServerHandle;
use std::sync::{Arc, Mutex};

type ShutdownHandle = Arc<Mutex<Option<ServerHandle>>>;

use engine::WasmtimeConfig;
use registry::ModuleRegistry;
use services::ServiceRegistry;
use wasm_module::{ModuleContext, ModuleProperties, Response, ServiceKind, WasmModule};

// ---------------------------------------------------------------------------
// Example modules
// ---------------------------------------------------------------------------

struct UserModule;

impl WasmModule for UserModule {
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.export("get_name");
        let pg = ctx.postgres.clone();
        let redis = ctx.redis.clone();
        let s3 = ctx.s3.clone();
        let call_mod = ctx.call_module.clone();

        ctx.get("/", || Response::ok("User Module — /user/"))
           .get("/list", move || {
               let rows = pg.as_ref().unwrap()
                   .query("SELECT id, name FROM users")
                   .unwrap_or_else(|e| format!(r#"{{"error":"{}"}}"#, e));
               Response::json(rows.into_bytes())
           })
           .get("/cache", move || {
               let val = redis.as_ref().unwrap()
                   .get("homepage:stats").unwrap()
                   .unwrap_or_else(|| "not cached".into());
               Response::ok(val)
           })
           .get("/files", move || {
               let data = s3.as_ref().unwrap()
                   .get("my-bucket", "hello.txt")
                   .unwrap_or_else(|e| e.as_bytes().to_vec());
               Response::ok(String::from_utf8_lossy(&data).to_string())
           })
           .get("/from-order", move || {
               use wasm_module::FromModuleBytes;
               let bytes = call_mod.as_ref().unwrap()("order", "get_info", b"{}");
               Response::json(String::from_module_bytes(&bytes).unwrap_or_default().into_bytes())
           });
    }

    fn properties(&self) -> ModuleProperties {
        ModuleProperties {
            memory_pages: 2,
            required_services: vec![
                wasm_module::ServiceRequirement { kind: ServiceKind::Postgres, identifier: "main_db".into() },
                wasm_module::ServiceRequirement { kind: ServiceKind::Redis, identifier: "cache".into() },
                wasm_module::ServiceRequirement { kind: ServiceKind::S3, identifier: "assets".into() },
            ],
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

struct OrderModule;

impl WasmModule for OrderModule {
    fn register(&self, ctx: &mut ModuleContext) {
        ctx.export("get_info");
        let call_mod = ctx.call_module.clone();

        ctx.get("/", || Response::ok("Order Module — /order/"))
           .get("/call-user", move || {
               use wasm_module::FromModuleBytes;
               let bytes = call_mod.as_ref().unwrap()("user", "get_name", b"{}");
               Response::json(String::from_module_bytes(&bytes).unwrap_or_default().into_bytes())
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
    println!("╚══════════════════════════════════════════════╝");

    let wasm_config = WasmtimeConfig::default().build();
    let _engine = wasmtime::Engine::new(&wasm_config).expect("failed to create wasmtime engine");
    println!("[kernel] wasmtime engine ready");

    let registry = Arc::new(Mutex::new(ModuleRegistry::new()));
    let shutdown_handle: ShutdownHandle = Arc::new(Mutex::new(None));

    // -- Connect providers, register in ServiceRegistry -----------------------
    let mut service_registry = ServiceRegistry::new();

    // Postgres
    let pg_config = providers::postgres::PostgresConfig {
        url: std::env::var("DATABASE_URL").unwrap_or_default(),
        ..Default::default()
    };
    match providers::postgres::PostgresProvider::connect(pg_config).await {
        Ok(pg) => {
            service_registry.register_service("postgres", "main_db", pg);
            println!("[services] postgres/main_db connected");
        }
        Err(e) => {
            eprintln!("[services] postgres: {e} (using echo fallback)");
            service_registry.register_service("postgres", "main_db", EchoProvider("postgres"));
        }
    }

    // Redis
    let redis_config = providers::redis_provider::RedisConfig {
        url: std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
        ..Default::default()
    };
    let redis_result = tokio::task::spawn_blocking(move || {
        providers::redis_provider::RedisProvider::connect(redis_config)
    }).await.unwrap();
    match redis_result {
        Ok(r) => {
            service_registry.register_service("redis", "cache", r);
            println!("[services] redis/cache connected");
        }
        Err(e) => {
            eprintln!("[services] redis: {e} (using echo fallback)");
            service_registry.register_service("redis", "cache", EchoProvider("redis"));
        }
    }

    // MySQL
    let mysql_config = providers::mysql::MySqlConfig {
        url: std::env::var("MYSQL_URL").unwrap_or_default(),
        ..Default::default()
    };
    match providers::mysql::MySqlProvider::connect(mysql_config).await {
        Ok(my) => {
            service_registry.register_service("mysql", "main_db", my);
            println!("[services] mysql/main_db connected");
        }
        Err(e) => {
            eprintln!("[services] mysql: {e} (using echo fallback)");
            service_registry.register_service("mysql", "main_db", EchoProvider("mysql"));
        }
    }

    // S3
    let s3_config = providers::s3::S3Config {
        endpoint: std::env::var("S3_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".into()),
        access_key: std::env::var("S3_KEY").unwrap_or_default(),
        secret_key: std::env::var("S3_SECRET").unwrap_or_default(),
        region: std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".into()),
        force_path_style: true,
        ..Default::default()
    };
    match providers::s3::S3Provider::connect(s3_config) {
        Ok(s3) => {
            service_registry.register_service("s3", "assets", s3);
            println!("[services] s3/assets connected");
        }
        Err(e) => {
            eprintln!("[services] s3: {e} (using echo fallback)");
            service_registry.register_service("s3", "assets", EchoProvider("s3"));
        }
    }

    // HTTP
    match providers::http_client::HttpProvider::connect(Default::default()) {
        Ok(h) => {
            service_registry.register_service("http", "default", h);
            println!("[services] http/default connected");
        }
        Err(e) => {
            eprintln!("[services] http: {e} (using echo fallback)");
            service_registry.register_service("http", "default", EchoProvider("http"));
        }
    }

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
                            if let Ok(mut reg) = watcher_registry.lock() { reg.remove(&name); }
                        }
                    }
                }
            }
            Err(e) => eprintln!("[watcher] failed: {e}"),
        }
    });

    // -- Deploy modules -------------------------------------------------------
    {
        let mut reg = registry.lock().unwrap();
        let mut svc = service_registry.lock().unwrap();

        let user_mod: Arc<dyn WasmModule> = Arc::new(UserModule);
        let mut user_ctx = build_module_context(Arc::clone(&service_registry));
        user_mod.register(&mut user_ctx);
        svc.register_exports("user", &user_ctx, Arc::clone(&user_mod));
        reg.deploy("user", user_ctx, (1, 0, 0), Some(user_mod));
        println!("[deploy] user v1.0.0 → /user/*");

        let order_mod: Arc<dyn WasmModule> = Arc::new(OrderModule);
        let mut order_ctx = build_module_context(Arc::clone(&service_registry));
        order_mod.register(&mut order_ctx);
        svc.register_exports("order", &order_ctx, Arc::clone(&order_mod));
        reg.deploy("order", order_ctx, (1, 0, 0), Some(order_mod));
        println!("[deploy] order v1.0.0 → /order/*");
    }

    // -- Example scope --------------------------------------------------------
    let mut example_ctx = ModuleContext::new();
    example_ctx
        .get("/", || Response::ok("Hello from the dynamic scope!"))
        .get("/health", || Response::json(r#"{"status":"ok"}"#.as_bytes().to_vec()));
    let example_ctx = Arc::new(example_ctx);

    // -- Server ---------------------------------------------------------------
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

    // Typed handles — thin wrappers around ServiceRegistry
    ctx.postgres = Some(Arc::new(ServiceRegistryPostgresHandle(Arc::clone(&svc))));
    ctx.redis    = Some(Arc::new(ServiceRegistryRedisHandle(Arc::clone(&svc))));
    ctx.mysql    = Some(Arc::new(ServiceRegistryMySqlHandle(Arc::clone(&svc))));
    ctx.s3       = Some(Arc::new(ServiceRegistryS3Handle(Arc::clone(&svc))));
    ctx.http     = Some(Arc::new(ServiceRegistryHttpHandle(Arc::clone(&svc))));

    ctx
}

// ---------------------------------------------------------------------------
// Echo fallback provider (used when real services are unavailable)
// ---------------------------------------------------------------------------

use services::ServiceProvider;

struct EchoProvider(&'static str);

impl ServiceProvider for EchoProvider {
    fn call(&self, method: &str, payload: &[u8]) -> Vec<u8> {
        let body = String::from_utf8_lossy(payload);
        println!("[{}/echo] {}", self.0, body);
        serde_json::json!({
            "status": "ok",
            "service": format!("{}/echo", self.0),
            "rows": [],
            "value": null,
            "deleted": 0,
            "exists": false
        })
        .to_string()
        .into_bytes()
    }
}

// ---------------------------------------------------------------------------
// Typed handle wrappers — delegate to ServiceRegistry
// ---------------------------------------------------------------------------

use wasm_module::{PostgresHandle, RedisHandle, MySqlHandle, S3Handle, HttpHandle};

struct ServiceRegistryPostgresHandle(Arc<Mutex<ServiceRegistry>>);
impl PostgresHandle for ServiceRegistryPostgresHandle {
    fn query(&self, sql: &str) -> Result<String, String> {
        let payload = serde_json::json!({ "kind": "query", "sql": sql }).to_string();
        let bytes = self.0.lock().unwrap().call_service("postgres", "main_db", "", payload.as_bytes());
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
    fn execute(&self, sql: &str) -> Result<u64, String> {
        let payload = serde_json::json!({ "kind": "execute", "sql": sql }).to_string();
        let bytes = self.0.lock().unwrap().call_service("postgres", "main_db", "", payload.as_bytes());
        let s = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        v["rows_affected"].as_u64().ok_or("missing rows_affected".into())
    }
    fn query_with(&self, sql: &str, params: &[&str]) -> Result<String, String> {
        let payload = serde_json::json!({ "kind": "query_with", "sql": sql, "params": params }).to_string();
        let bytes = self.0.lock().unwrap().call_service("postgres", "main_db", "", payload.as_bytes());
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
}

struct ServiceRegistryRedisHandle(Arc<Mutex<ServiceRegistry>>);
impl RedisHandle for ServiceRegistryRedisHandle {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        let payload = serde_json::json!({ "cmd": "GET", "key": key }).to_string();
        let bytes = self.0.lock().unwrap().call_service("redis", "cache", "", payload.as_bytes());
        let s = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        Ok(v["value"].as_str().map(|s| s.to_string()))
    }
    fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), String> {
        let payload = serde_json::json!({ "cmd": "SET", "key": key, "value": value, "ttl": ttl }).to_string();
        self.0.lock().unwrap().call_service("redis", "cache", "", payload.as_bytes());
        Ok(())
    }
    fn del(&self, keys: &[&str]) -> Result<u64, String> {
        let payload = serde_json::json!({ "cmd": "DEL", "keys": keys }).to_string();
        let bytes = self.0.lock().unwrap().call_service("redis", "cache", "", payload.as_bytes());
        let s = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        v["deleted"].as_u64().ok_or("missing deleted".into())
    }
    fn incr(&self, key: &str, amount: Option<i64>) -> Result<i64, String> {
        let amt = amount.unwrap_or(1);
        let payload = serde_json::json!({ "cmd": "INCRBY", "key": key, "amount": amt }).to_string();
        let bytes = self.0.lock().unwrap().call_service("redis", "cache", "", payload.as_bytes());
        let s = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        v["value"].as_i64().ok_or("missing value".into())
    }
    fn exists(&self, key: &str) -> Result<bool, String> {
        let payload = serde_json::json!({ "cmd": "EXISTS", "key": key }).to_string();
        let bytes = self.0.lock().unwrap().call_service("redis", "cache", "", payload.as_bytes());
        let s = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        Ok(v["exists"].as_bool().unwrap_or(false))
    }
}

struct ServiceRegistryMySqlHandle(Arc<Mutex<ServiceRegistry>>);
impl MySqlHandle for ServiceRegistryMySqlHandle {
    fn query(&self, sql: &str) -> Result<String, String> {
        let payload = serde_json::json!({ "kind": "query", "sql": sql }).to_string();
        let bytes = self.0.lock().unwrap().call_service("mysql", "main_db", "", payload.as_bytes());
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
    fn execute(&self, sql: &str) -> Result<u64, String> {
        let payload = serde_json::json!({ "kind": "execute", "sql": sql }).to_string();
        let bytes = self.0.lock().unwrap().call_service("mysql", "main_db", "", payload.as_bytes());
        let s = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        v["rows_affected"].as_u64().ok_or("missing rows_affected".into())
    }
    fn query_with(&self, sql: &str, params: &[&str]) -> Result<String, String> {
        let payload = serde_json::json!({ "kind": "query_with", "sql": sql, "params": params }).to_string();
        let bytes = self.0.lock().unwrap().call_service("mysql", "main_db", "", payload.as_bytes());
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
}

struct ServiceRegistryS3Handle(Arc<Mutex<ServiceRegistry>>);
impl S3Handle for ServiceRegistryS3Handle {
    fn put(&self, bucket: &str, key: &str, data: &[u8]) -> Result<String, String> {
        let payload = serde_json::json!({ "cmd": "PUT", "bucket": bucket, "key": key, "size": data.len() }).to_string();
        let bytes = self.0.lock().unwrap().call_service("s3", "assets", "", payload.as_bytes());
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
    fn get(&self, bucket: &str, key: &str) -> Result<Vec<u8>, String> {
        let payload = serde_json::json!({ "cmd": "GET", "bucket": bucket, "key": key }).to_string();
        Ok(self.0.lock().unwrap().call_service("s3", "assets", "", payload.as_bytes()))
    }
    fn delete(&self, bucket: &str, key: &str) -> Result<bool, String> {
        let payload = serde_json::json!({ "cmd": "DELETE", "bucket": bucket, "key": key }).to_string();
        let bytes = self.0.lock().unwrap().call_service("s3", "assets", "", payload.as_bytes());
        let s = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        Ok(v["deleted"].as_bool().unwrap_or(false))
    }
    fn list(&self, bucket: &str, prefix: Option<&str>) -> Result<String, String> {
        let payload = serde_json::json!({ "cmd": "LIST", "bucket": bucket, "prefix": prefix }).to_string();
        let bytes = self.0.lock().unwrap().call_service("s3", "assets", "", payload.as_bytes());
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
}

struct ServiceRegistryHttpHandle(Arc<Mutex<ServiceRegistry>>);
impl HttpHandle for ServiceRegistryHttpHandle {
    fn get(&self, url: &str) -> Result<String, String> {
        let payload = serde_json::json!({ "method": "GET", "url": url }).to_string();
        let bytes = self.0.lock().unwrap().call_service("http", "default", "", payload.as_bytes());
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
    fn post(&self, url: &str, body: &str) -> Result<String, String> {
        let payload = serde_json::json!({ "method": "POST", "url": url, "body": body }).to_string();
        let bytes = self.0.lock().unwrap().call_service("http", "default", "", payload.as_bytes());
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
    fn put(&self, url: &str, body: &str) -> Result<String, String> {
        let payload = serde_json::json!({ "method": "PUT", "url": url, "body": body }).to_string();
        let bytes = self.0.lock().unwrap().call_service("http", "default", "", payload.as_bytes());
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
    fn delete(&self, url: &str) -> Result<String, String> {
        let payload = serde_json::json!({ "method": "DELETE", "url": url }).to_string();
        let bytes = self.0.lock().unwrap().call_service("http", "default", "", payload.as_bytes());
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
}
