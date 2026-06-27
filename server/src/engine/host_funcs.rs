//! Host functions exposed to WASM modules via the wasmtime [`Linker`].

use std::sync::Arc;
use wasm_module::ModuleContext;
use wasmtime::{Caller, Linker, Memory};

use super::super::registry::ModuleRegistry;
use super::super::services::ServiceRegistry;

// ---------------------------------------------------------------------------
// HostState
// ---------------------------------------------------------------------------

pub struct HostState {
    pub registry: Arc<std::sync::Mutex<ModuleRegistry>>,
    pub services: Arc<std::sync::Mutex<ServiceRegistry>>,
    pub current_ctx: ModuleContext,
}

impl HostState {
    pub fn new(
        registry: Arc<std::sync::Mutex<ModuleRegistry>>,
        services: Arc<std::sync::Mutex<ServiceRegistry>>,
    ) -> Self {
        Self {
            registry,
            services,
            current_ctx: ModuleContext::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Linker setup
// ---------------------------------------------------------------------------

pub fn setup_linker(
    linker: &mut Linker<HostState>,
) -> Result<(), Box<dyn std::error::Error>> {
    // -- register_route(method_ptr, method_len, path_ptr, path_len) ---------
    linker.func_wrap(
        "env",
        "register_route",
        |mut caller: Caller<'_, HostState>,
         method_ptr: i32,
         method_len: i32,
         path_ptr: i32,
         path_len: i32| {
            let mem = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => { eprintln!("[wasm] missing memory"); return; }
            };
            let method = read_string(&caller, &mem, method_ptr as usize, method_len as usize);
            let path = read_string(&caller, &mem, path_ptr as usize, path_len as usize);
            if let (Some(method), Some(path)) = (method, path) {
                let state = caller.data_mut();
                match method.to_uppercase().as_str() {
                    "GET"  => { state.current_ctx.get(&path, || wasm_module::Response::ok("wasm")); }
                    "POST" => { state.current_ctx.post(&path, || wasm_module::Response::ok("wasm")); }
                    _ => eprintln!("[wasm] unsupported method: {method}"),
                }
            }
        },
    )?;

    // -- call_service(payload_ptr, payload_len) -> result_len ---------------
    // Payload: JSON with { "kind", "id", "payload" }
    // Returns: length of result written to WASM memory at offset 0
    linker.func_wrap(
        "env",
        "call_service",
        |mut caller: Caller<'_, HostState>,
         payload_ptr: i32,
         payload_len: i32|
         -> i32 {
            let mem = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => { eprintln!("[wasm] missing memory"); return 0; }
            };

            let payload = read_string(&caller, &mem, payload_ptr as usize, payload_len as usize);
            let payload = match payload {
                Some(p) => p,
                None => return 0,
            };

            // Parse: {"kind":"postgres","id":"main_db","payload":"..."}
            let v: serde_json::Value = match serde_json::from_str(&payload) {
                Ok(v) => v,
                Err(_) => return 0,
            };

            let kind = v["kind"].as_str().unwrap_or("");
            let id = v["id"].as_str().unwrap_or("");
            let svc_payload = v["payload"].as_str().unwrap_or("");

            let state = caller.data();
            let result = state.services.lock().unwrap()
                .call_service(kind, id, "", svc_payload.as_bytes());

            // Write result to WASM memory at offset 0
            let data = mem.data(&caller);
            let len = result.len().min(data.len());
            // We need mutable access — use unsafe write
            unsafe {
                let ptr = mem.data_ptr(&caller);
                std::ptr::copy_nonoverlapping(result.as_ptr(), ptr, len);
            }
            len as i32
        },
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_string(
    caller: &Caller<'_, HostState>,
    mem: &Memory,
    ptr: usize,
    len: usize,
) -> Option<String> {
    let data = mem.data(caller);
    let slice = data.get(ptr..ptr + len)?;
    String::from_utf8(slice.to_vec()).ok()
}
