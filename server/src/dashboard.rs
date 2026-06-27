//! Dashboard — API endpoints and static UI for module management.
//!
//! ## Endpoints
//!
//! | Method | Path | Purpose |
//! |--------|------|---------|
//! | `GET` | `/dashboard` | Serve the dashboard HTML |
//! | `GET` | `/api/modules` | List all modules |
//! | `POST` | `/api/modules/deploy` | Deploy a module |
//! | `POST` | `/api/modules/{name}/swap` | Swap blue ↔ green |
//! | `DELETE` | `/api/modules/{name}` | Remove a module |
//! | `POST` | `/api/shutdown/graceful` | Graceful shutdown (drain requests) |
//! | `POST` | `/api/shutdown/force` | Force shutdown (kill immediately) |

use actix_web::{web, HttpResponse};
use std::sync::{Arc, Mutex};

use crate::registry::ModuleRegistry;
use crate::ShutdownHandle;

// ---------------------------------------------------------------------------
// Dashboard HTML
// ---------------------------------------------------------------------------

const DASHBOARD_HTML: &str = include_str!("../static/dashboard.html");

// ---------------------------------------------------------------------------
// API handlers
// ---------------------------------------------------------------------------

async fn api_list_modules(registry: web::Data<Arc<Mutex<ModuleRegistry>>>) -> HttpResponse {
    let reg = registry.lock().unwrap();
    HttpResponse::Ok().json(reg.snapshot())
}

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

async fn api_deploy_module(_registry: web::Data<Arc<Mutex<ModuleRegistry>>>) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "message": "Deploy endpoint ready — upload a .wasm file via multipart form (demo placeholder)",
        "form_field": "module",
        "example": "curl -F module=@user.wasm http://localhost:8080/api/modules/deploy"
    }))
}

// ---------------------------------------------------------------------------
// Shutdown handlers
// ---------------------------------------------------------------------------

/// Graceful shutdown: stop accepting new connections, finish in-flight
/// requests, then exit cleanly.
async fn api_shutdown_graceful(handle: web::Data<ShutdownHandle>) -> HttpResponse {
    let mut guard = handle.lock().unwrap();
    if let Some(h) = guard.take() {
        println!("[kernel] graceful shutdown initiated — draining requests...");
        // Spawn the actual stop so this request can return a response first
        tokio::spawn(async move {
            h.stop(true).await;
            println!("[kernel] server stopped gracefully");
        });
        HttpResponse::Ok().json(serde_json::json!({
            "shutdown": "graceful",
            "message": "Server shutting down — finishing in-flight requests before exit."
        }))
    } else {
        HttpResponse::Gone().json(serde_json::json!({
            "error": "shutdown already in progress"
        }))
    }
}

/// Force shutdown: terminate immediately. In-flight requests are aborted.
async fn api_shutdown_force(handle: web::Data<ShutdownHandle>) -> HttpResponse {
    let mut guard = handle.lock().unwrap();
    if let Some(h) = guard.take() {
        println!("[kernel] force shutdown initiated — killing immediately...");
        tokio::spawn(async move {
            h.stop(false).await;
            println!("[kernel] server force-stopped");
        });
        HttpResponse::Ok().json(serde_json::json!({
            "shutdown": "force",
            "message": "Server killed immediately — in-flight requests were aborted."
        }))
    } else {
        HttpResponse::Gone().json(serde_json::json!({
            "error": "shutdown already in progress"
        }))
    }
}

// ---------------------------------------------------------------------------
// Dashboard page
// ---------------------------------------------------------------------------

async fn dashboard_page() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(DASHBOARD_HTML)
}

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/modules", web::get().to(api_list_modules))
            .route("/modules/deploy", web::post().to(api_deploy_module))
            .route("/modules/{name}/swap", web::post().to(api_swap_module))
            .route("/modules/{name}", web::delete().to(api_remove_module))
            .route("/shutdown/graceful", web::post().to(api_shutdown_graceful))
            .route("/shutdown/force", web::post().to(api_shutdown_force)),
    )
    .route("/dashboard", web::get().to(dashboard_page));
}
