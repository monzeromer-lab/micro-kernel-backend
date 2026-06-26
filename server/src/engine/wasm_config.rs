//! WASM engine configuration — wraps wasmtime [`Config`] with a builder
//! that applies [`wasm_module::ModuleProperties`] from loaded modules.

pub struct WasmtimeConfig {
    inner: wasmtime::Config,
}

impl WasmtimeConfig {
    pub fn new() -> Self {
        let mut config = wasmtime::Config::new();

        config.wasm_bulk_memory(true);
        config.wasm_multi_value(true);
        config.wasm_multi_memory(true);
        config.wasm_reference_types(true);
        config.wasm_simd(true);
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);
        config.epoch_interruption(true);
        config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
        config.memory_reservation(0x10000);

        Self { inner: config }
    }

    /// Apply a module's properties to this config before compilation.
    pub fn apply_module_properties(
        &mut self,
        props: &wasm_module::ModuleProperties,
    ) -> &mut Self {
        if props.memory64 {
            self.inner.wasm_memory64(true);
        }
        if props.consume_fuel {
            self.inner.consume_fuel(true);
        }
        if let Some(stack) = props.max_wasm_stack {
            self.inner.max_wasm_stack(stack);
        }
        self
    }

    pub fn build(self) -> wasmtime::Config {
        self.inner
    }

    // -- Convenience setters (used by dashboard to tune the "kernel") --------

    pub fn opt_level(mut self, level: wasmtime::OptLevel) -> Self {
        self.inner.cranelift_opt_level(level);
        self
    }

    pub fn memory64(mut self, enable: bool) -> Self {
        self.inner.wasm_memory64(enable);
        self
    }

    pub fn max_wasm_stack(mut self, size: usize) -> Self {
        self.inner.max_wasm_stack(size);
        self
    }

    pub fn consume_fuel(mut self, enable: bool) -> Self {
        self.inner.consume_fuel(enable);
        self
    }

    pub fn memory_guard_size(mut self, bytes: u64) -> Self {
        self.inner.memory_guard_size(bytes);
        self
    }

    pub fn memory_reservation(mut self, bytes: u64) -> Self {
        self.inner.memory_reservation(bytes);
        self
    }

    pub fn wasm_threads(mut self, enable: bool) -> Self {
        self.inner.wasm_threads(enable);
        self
    }

    pub fn debug_info(mut self, enable: bool) -> Self {
        self.inner.debug_info(enable);
        self
    }
}

impl Default for WasmtimeConfig {
    fn default() -> Self {
        Self::new()
    }
}
