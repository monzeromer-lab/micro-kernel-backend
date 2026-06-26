//! Dashboard — API endpoints and static UI for module management.
//!
//! ## Endpoints
//!
//! | Method | Path | Purpose |
//! |--------|------|---------|
//! | `GET` | `/dashboard` | Serve the dashboard HTML |
//! | `GET` | `/api/modules` | List all modules (blue/green status) |
//! | `POST` | `/api/modules/deploy` | Deploy/update a module |
//! | `POST` | `/api/modules/{name}/swap` | Swap blue ↔ green |
//! | `DELETE` | `/api/modules/{name}` | Remove a module |

use actix_web::{web, HttpResponse};
use std::sync::{Arc, Mutex};

use crate::registry::ModuleRegistry;

// ---------------------------------------------------------------------------
// Dashboard HTML (inlined — no extra static files for the demo)
// ---------------------------------------------------------------------------

const DASHBOARD_HTML: &str = include_str!("../static/dashboard.html");

// ---------------------------------------------------------------------------
// API handlers
// ---------------------------------------------------------------------------

/// GET /api/modules — list all modules.
async fn api_list_modules(
    registry: web::Data<Arc<Mutex<ModuleRegistry>>>,
) -> HttpResponse {
    let reg = registry.lock().unwrap();
    HttpResponse::Ok().json(reg.snapshot())
}

/// DELETE /api/modules/{name} — remove a module.
async fn api_remove_module(
    registry: web::Data<Arc<Mutex<ModuleRegistry>>>,
    path: web::Path<String>,
) -> HttpResponse {
    let name = path.into_inner();
    let mut reg = registry.lock().unwrap();
    if reg.remove(&name) {
        HttpResponse::Ok().json(serde_json::json!({"removed": name}))
    } else {
        HttpResponse::NotFound().json(serde_json::json!({"error": "not found"}))
    }
}

/// POST /api/modules/{name}/swap — swap blue ↔ green.
async fn api_swap_module(
    registry: web::Data<Arc<Mutex<ModuleRegistry>>>,
    path: web::Path<String>,
) -> HttpResponse {
    let name = path.into_inner();
    let mut reg = registry.lock().unwrap();
    match reg.swap(&name) {
        Some(new_active) => HttpResponse::Ok().json(serde_json::json!({
            "swapped": name,
            "active": new_active
        })),
        None => HttpResponse::BadRequest().json(serde_json::json!({
            "error": "cannot swap — inactive slot is empty"
        })),
    }
}

/// GET /dashboard — serve the dashboard HTML page.
async fn dashboard_page() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(DASHBOARD_HTML)
}

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

/// Register all dashboard routes on the provided [`ServiceConfig`].
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/modules", web::get().to(api_list_modules))
            .route("/modules/deploy", web::post().to(api_deploy_module))
            .route("/modules/{name}/swap", web::post().to(api_swap_module))
            .route("/modules/{name}", web::delete().to(api_remove_module)),
    )
    .route("/dashboard", web::get().to(dashboard_page));
}

// ---------------------------------------------------------------------------
// POST /api/modules/deploy — deploy a module (placeholder for demo)
// ---------------------------------------------------------------------------

/// For the tech talk demo, this is a placeholder showing the API shape.
/// The real implementation would receive a `.wasm` file via multipart upload,
/// compile it with wasmtime, instantiate it, and call `WasmModule::register()`.
async fn api_deploy_module(
    registry: web::Data<Arc<Mutex<ModuleRegistry>>>,
) -> HttpResponse {
    // In a real deployment, this would:
    // 1. Read the uploaded .wasm bytes from the multipart form
    // 2. Compile with wasmtime::Module::new()
    // 3. Create a Store<HostState> and Linker
    // 4. Instantiate, call init(), collect ModuleContext
    // 5. Call registry.deploy(name, ctx, version)

    HttpResponse::Ok().json(serde_json::json!({
        "message": "Deploy endpoint ready — upload a .wasm file via multipart form (demo placeholder)",
        "form_field": "module",
        "example": "curl -F module=@user.wasm http://localhost:8080/api/modules/deploy"
    }))
}
