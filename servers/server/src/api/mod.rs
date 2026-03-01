use axum::Router;

use crate::AppState;

pub mod channels;
pub mod messages;
pub mod servers;
pub mod upload;
pub mod users;

/// Main API router. Auth middleware is applied by `main.rs` via `route_layer`
/// after the `AppState` is constructed, to avoid the bootstrapping problem of
/// needing state before state exists.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(users::router())
        .merge(servers::router())
        .merge(channels::router())
        .merge(messages::router())
        .merge(upload::router())
}
