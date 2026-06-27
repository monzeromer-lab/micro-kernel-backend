//! Service Registry — mediates external-service access and inter-module calls.
//!
//! This is the kernel component that:
//! - Holds database pools, HTTP clients, Redis connections
//! - Routes `call_service("postgres", "main_db", sql)` → actual DB query
//! - Routes `call_module("user", "get_name", args)` → Module A's export handler
//!
//! Modules never talk to each other or to external services directly.
//! Everything goes through the kernel.

use std::collections::HashMap;
use std::sync::Arc;
use wasm_module::WasmModule;

// ---------------------------------------------------------------------------
// ServiceProvider trait
// ---------------------------------------------------------------------------

/// Something the kernel can call on behalf of a module.
///
/// Implementations wrap database pools, HTTP clients, Redis connections, etc.
pub trait ServiceProvider: Send + Sync {
    /// Execute a call against this service.
    ///
    /// `method` is service-specific: for Postgres it's a SQL string, for HTTP
    /// it's a URL path, for Redis it's a command name.
    fn call(&self, method: &str, payload: &[u8]) -> Vec<u8>;
}

// ---------------------------------------------------------------------------
// ServiceRegistry
// ---------------------------------------------------------------------------

pub struct ServiceRegistry {
    /// External services, keyed by `"kind/identifier"` (e.g. `"postgres/main_db"`).
    services: HashMap<String, Box<dyn ServiceProvider>>,
    /// Module export handlers, keyed by `"module_name::function_name"`.
    exports: HashMap<String, ExportEntry>,
}

struct ExportEntry {
    /// The module instance — we call [`WasmModule::on_export_call`] on it.
    module: Arc<dyn WasmModule>,
    /// The function name (redundant with the key, but convenient).
    function: String,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            exports: HashMap::new(),
        }
    }

    // -- External services --------------------------------------------------

    /// Register an external service provider.
    ///
    /// ```rust,ignore
    /// registry.register_service("postgres/main_db", PostgresProvider::new(pool));
    /// registry.register_service("http/default", HttpClientProvider::new(client));
    /// ```
    pub fn register_service(
        &mut self,
        kind: &str,
        identifier: &str,
        provider: impl ServiceProvider + 'static,
    ) {
        let key = format!("{kind}/{identifier}");
        self.services.insert(key, Box::new(provider));
    }

    /// Call an external service from a module.
    ///
    /// Modules invoke this through `ctx.call_service` (which the host wires up).
    pub fn call_service(&self, kind: &str, identifier: &str, method: &str, payload: &[u8]) -> Vec<u8> {
        let key = format!("{kind}/{identifier}");
        match self.services.get(&key) {
            Some(svc) => svc.call(method, payload),
            None => {
                eprintln!("[services] unknown service: {key}");
                format!("error: service '{key}' not registered").into_bytes()
            }
        }
    }

    // -- Module exports -----------------------------------------------------

    /// Register a module's exported functions so other modules can call them.
    ///
    /// Called by the host after `WasmModule::register()` completes.
    pub fn register_exports(
        &mut self,
        module_name: &str,
        ctx: &wasm_module::ModuleContext,
        module: Arc<dyn WasmModule>,
    ) {
        for func in ctx.exports() {
            let key = format!("{module_name}::{func}");
            self.exports.insert(
                key,
                ExportEntry {
                    module: Arc::clone(&module),
                    function: func.clone(),
                },
            );
        }
    }

    /// Remove all exports for a module (called when module is unloaded).
    pub fn remove_exports(&mut self, module_name: &str) {
        let prefix = format!("{module_name}::");
        self.exports.retain(|key, _| !key.starts_with(&prefix));
    }

    /// Call a function exported by another module.
    ///
    /// Modules invoke this through `ctx.call_module` (which the host wires up).
    pub fn call_export(
        &self,
        module_name: &str,
        function: &str,
        args: &[u8],
    ) -> Vec<u8> {
        let key = format!("{module_name}::{function}");
        match self.exports.get(&key) {
            Some(entry) => entry.module.on_export_call(&entry.function, args),
            None => {
                eprintln!("[services] unknown export: {key}");
                format!("error: export '{key}' not found").into_bytes()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in service providers (demo placeholders)
// ---------------------------------------------------------------------------

/// A demo Postgres provider that echoes back the SQL.
pub struct PostgresProvider {
    pub label: String,
}

impl ServiceProvider for PostgresProvider {
    fn call(&self, _method: &str, payload: &[u8]) -> Vec<u8> {
        let sql = String::from_utf8_lossy(payload);
        println!("[pg/{}] SQL: {}", self.label, sql);
        // Placeholder: return a static result
        format!(r#"{{"rows":[],"service":"postgres/{}"}}"#, self.label).into_bytes()
    }
}

/// A demo HTTP provider that echoes back the request.
pub struct HttpClientProvider;

impl ServiceProvider for HttpClientProvider {
    fn call(&self, method: &str, payload: &[u8]) -> Vec<u8> {
        let body = String::from_utf8_lossy(payload);
        println!("[http] {} {}", method, body);
        format!(r#"{{"status":200,"body":"echo: {}"}}"#, body).into_bytes()
    }
}

/// A demo Redis provider that echoes back the command.
pub struct RedisProvider {
    pub label: String,
}

impl ServiceProvider for RedisProvider {
    fn call(&self, method: &str, payload: &[u8]) -> Vec<u8> {
        let cmd = String::from_utf8_lossy(payload);
        println!("[redis/{}] {} {}", self.label, method, cmd);
        format!(r#"{{"result":"ok","service":"redis/{}"}}"#, self.label).into_bytes()
    }
}
