//! Service Providers — real implementations for Postgres, MySQL, Redis, S3, HTTP.
//!
//! Each provider exposes a full configuration surface and implements both
//! [`ServiceProvider`] and the typed handle trait from `wasm-module`.

pub mod postgres;
pub mod mysql;
pub mod redis_provider;
pub mod s3;
pub mod http_client;
