//! Dynamic Resource — a REST-style resource within a scope.
//!
//! Wraps [`wasm_module::ModuleContext`] route definitions for a single
//! URL path.

use actix_web::web;

pub struct Resource {
    path: String,
}

impl Resource {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    pub fn configure(&self, cfg: &mut web::ServiceConfig) {
        cfg.service(web::resource(&self.path));
    }
}
