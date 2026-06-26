//! Bridge: converts [`wasm_module::ModuleContext`] into Actix [`ServiceConfig`].
//!
//! When a WASM module calls `ctx.get("/path", handler)`, the host collects
//! those definitions and this module converts them into live Actix routes.

use actix_web::web;
use actix_web::HttpResponse;
use std::sync::Arc;
use wasm_module::{Method, ModuleContext};

// ---------------------------------------------------------------------------
// Context → Actix
// ---------------------------------------------------------------------------

pub fn mount_context(cfg: &mut web::ServiceConfig, ctx: &ModuleContext) {
    for route in ctx.routes() {
        let path = route.path.clone();
        let rayna_response = Arc::new(route.handler.call());

        match route.method {
            Method::Get => {
                let resp = Arc::clone(&rayna_response);
                cfg.route(&path, web::get().to(move || {
                    let r = Arc::clone(&resp);
                    async move { convert_response(&r) }
                }));
            }
            Method::Post => {
                let resp = Arc::clone(&rayna_response);
                cfg.route(&path, web::post().to(move || {
                    let r = Arc::clone(&resp);
                    async move { convert_response(&r) }
                }));
            }
            Method::Put => {
                let resp = Arc::clone(&rayna_response);
                cfg.route(&path, web::put().to(move || {
                    let r = Arc::clone(&resp);
                    async move { convert_response(&r) }
                }));
            }
            Method::Delete => {
                let resp = Arc::clone(&rayna_response);
                cfg.route(&path, web::delete().to(move || {
                    let r = Arc::clone(&resp);
                    async move { convert_response(&r) }
                }));
            }
            Method::Patch => {
                let resp = Arc::clone(&rayna_response);
                cfg.route(&path, web::patch().to(move || {
                    let r = Arc::clone(&resp);
                    async move { convert_response(&r) }
                }));
            }
        }
    }

    // Nested scopes are mounted in registry.rs via web::scope
}

// ---------------------------------------------------------------------------
// Response conversion
// ---------------------------------------------------------------------------

fn convert_response(resp: &wasm_module::Response) -> HttpResponse {
    let mut builder = HttpResponse::build(
        actix_web::http::StatusCode::from_u16(resp.status)
            .unwrap_or(actix_web::http::StatusCode::OK),
    );

    for (key, value) in &resp.headers {
        builder.insert_header((key.as_str(), value.as_str()));
    }

    builder.body(resp.body.clone())
}
