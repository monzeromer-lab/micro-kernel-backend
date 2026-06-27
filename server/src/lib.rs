//! Micro-kernel server — library interface for testing.

use actix_web::dev::ServerHandle;
use std::sync::{Arc, Mutex};

pub type ShutdownHandle = Arc<Mutex<Option<ServerHandle>>>;

pub mod dashboard;
pub mod engine;
pub mod guard;
pub mod middleware;
pub mod providers;
pub mod registry;
pub mod resource;
pub mod scope;
pub mod services;
pub mod watcher;
