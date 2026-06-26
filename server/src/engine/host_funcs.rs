//! Host functions exposed to WASM modules via the wasmtime [`Linker`].
//!
//! Modules import from the `"env"` namespace to register themselves.

use std::sync::Arc;
use wasm_module::ModuleContext;
use wasmtime::{Caller, Linker, Memory};

use super::super::registry::ModuleRegistry;

// ---------------------------------------------------------------------------
// HostState
// ---------------------------------------------------------------------------

pub struct HostState {
    pub registry: Arc<std::sync::Mutex<ModuleRegistry>>,
    pub current_ctx: ModuleContext,
}

impl HostState {
    pub fn new(registry: Arc<std::sync::Mutex<ModuleRegistry>>) -> Self {
        Self {
            registry,
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
                _ => {
                    eprintln!("[wasm] module missing 'memory' export");
                    return;
                }
            };

            let method = read_string(&caller, &mem, method_ptr as usize, method_len as usize);
            let path = read_string(&caller, &mem, path_ptr as usize, path_len as usize);

            if let (Some(method), Some(path)) = (method, path) {
                println!("[wasm] registered: {} {}", method.to_uppercase(), path);
                let state = caller.data_mut();
                match method.to_uppercase().as_str() {
                    "GET" => {
                        state
                            .current_ctx
                            .get(&path, || wasm_module::Response::ok("wasm handler"));
                    }
                    "POST" => {
                        state
                            .current_ctx
                            .post(&path, || wasm_module::Response::ok("wasm handler"));
                    }
                    _ => {
                        eprintln!("[wasm] unsupported method: {}", method);
                    }
                }
            }
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
