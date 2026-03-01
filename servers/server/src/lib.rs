//! Poly Server library — shared exports for main.rs and integration tests.

pub mod api;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod ws;

pub use config::Config;

use std::sync::Arc;

/// Shared application state threaded through all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<db::Db>,
    pub config: Arc<Config>,
    pub ws: Arc<ws::WsState>,
}
