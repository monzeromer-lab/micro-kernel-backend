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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wasm_module::{ModuleContext, WasmModule};

    struct TestProvider(&'static str);
    impl ServiceProvider for TestProvider {
        fn call(&self, _m: &str, payload: &[u8]) -> Vec<u8> {
            format!("{}:{}", self.0, String::from_utf8_lossy(payload)).into_bytes()
        }
    }

    #[test]
    fn register_and_call_service() {
        let mut svc = ServiceRegistry::new();
        svc.register_service("test", "demo", TestProvider("tp"));
        let r = svc.call_service("test", "demo", "", b"hello");
        assert_eq!(String::from_utf8(r).unwrap(), "tp:hello");
    }

    #[test]
    fn unknown_service_returns_error() {
        let svc = ServiceRegistry::new();
        let r = svc.call_service("no", "svc", "", b"hi");
        let s = String::from_utf8(r).unwrap();
        assert!(s.contains("error"));
    }

    #[test]
    fn multiple_services() {
        let mut svc = ServiceRegistry::new();
        svc.register_service("a", "1", TestProvider("A"));
        svc.register_service("b", "2", TestProvider("B"));
        assert_eq!(String::from_utf8(svc.call_service("a", "1", "", b"x")).unwrap(), "A:x");
        assert_eq!(String::from_utf8(svc.call_service("b", "2", "", b"y")).unwrap(), "B:y");
    }

    struct TestModule;
    impl WasmModule for TestModule {
        fn register(&self, ctx: &mut wasm_module::ModuleContext) {
            ctx.export("hello").export("world");
        }
        fn on_export_call(&self, f: &str, _: &[u8]) -> Vec<u8> {
            match f { "hello" => b"HELLO".to_vec(), "world" => b"WORLD".to_vec(), _ => vec![] }
        }
    }

    #[test]
    fn register_and_call_export() {
        let mut svc = ServiceRegistry::new();
        let mut ctx = ModuleContext::new();
        let m: Arc<dyn WasmModule> = Arc::new(TestModule);
        m.register(&mut ctx);
        svc.register_exports("mod", &ctx, m.clone());

        assert_eq!(svc.call_export("mod", "hello", b""), b"HELLO");
        assert_eq!(svc.call_export("mod", "world", b""), b"WORLD");
    }

    #[test]
    fn unknown_export_returns_error() {
        let svc = ServiceRegistry::new();
        let r = svc.call_export("mod", "nope", b"");
        assert!(String::from_utf8(r).unwrap().contains("not found"));
    }

    #[test]
    fn remove_exports_then_call_fails() {
        let mut svc = ServiceRegistry::new();
        let mut ctx = ModuleContext::new();
        let m: Arc<dyn WasmModule> = Arc::new(TestModule);
        m.register(&mut ctx);
        svc.register_exports("mod", &ctx, m.clone());

        assert!(!String::from_utf8(svc.call_export("mod", "hello", b"")).unwrap().contains("error"));
        svc.remove_exports("mod");
        assert!(String::from_utf8(svc.call_export("mod", "hello", b"")).unwrap().contains("not found"));
    }

    #[test]
    fn inter_module_cross_calling() {
        let mut svc = ServiceRegistry::new();
        let mut ctx_a = ModuleContext::new();
        let ma: Arc<dyn WasmModule> = Arc::new(TestModule);
        ma.register(&mut ctx_a);
        svc.register_exports("mod_a", &ctx_a, ma.clone());
        assert_eq!(svc.call_export("mod_a", "hello", b"{}"), b"HELLO");
        assert_eq!(svc.call_export("mod_a", "world", b"{}"), b"WORLD");
    }
}
