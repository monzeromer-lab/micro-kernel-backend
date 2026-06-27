//! Service Registry — mediates external-service access and inter-module calls.
//!
//! The kernel routes all module-to-service and module-to-module communication
//! through this registry.  Modules never talk to external services directly.

use std::collections::HashMap;
use std::sync::Arc;
use wasm_module::WasmModule;

// ---------------------------------------------------------------------------
// ServiceProvider trait
// ---------------------------------------------------------------------------

/// Something the kernel can call on behalf of a module.
pub trait ServiceProvider: Send + Sync {
    fn call(&self, method: &str, payload: &[u8]) -> Vec<u8>;
}

// ---------------------------------------------------------------------------
// ServiceRegistry
// ---------------------------------------------------------------------------

pub struct ServiceRegistry {
    services: HashMap<String, Box<dyn ServiceProvider>>,
    exports: HashMap<String, ExportEntry>,
}

struct ExportEntry {
    module: Arc<dyn WasmModule>,
    function: String,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            exports: HashMap::new(),
        }
    }

    pub fn register_service(
        &mut self,
        kind: &str,
        identifier: &str,
        provider: impl ServiceProvider + 'static,
    ) {
        let key = format!("{kind}/{identifier}");
        self.services.insert(key, Box::new(provider));
    }

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

    pub fn register_exports(
        &mut self,
        module_name: &str,
        ctx: &wasm_module::ModuleContext,
        module: Arc<dyn WasmModule>,
    ) {
        for func in ctx.exports() {
            let key = format!("{module_name}::{func}");
            self.exports.insert(key, ExportEntry {
                module: Arc::clone(&module),
                function: func.clone(),
            });
        }
    }

    pub fn remove_exports(&mut self, module_name: &str) {
        let prefix = format!("{module_name}::");
        self.exports.retain(|key, _| !key.starts_with(&prefix));
    }

    pub fn call_export(&self, module_name: &str, function: &str, args: &[u8]) -> Vec<u8> {
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
