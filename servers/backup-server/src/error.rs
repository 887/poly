//! Unified error type for the backup server.
//!
//! All handlers return `Result<T, AppError>`. The `IntoResponse` impl maps each
//! variant to the correct HTTP status code + JSON body `{ "error": "..." }`.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Typed application errors.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 400 Bad Request — invalid input from the client.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// 401 Unauthorized — missing or invalid auth token.
    #[error("unauthorized")]
    Unauthorized,

    /// 403 Forbidden — authenticated but not allowed (e.g. account limit reached).
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// 404 Not Found.
    #[error("not found")]
    NotFound,

    /// 409 Conflict — e.g. duplicate record.
    #[error("conflict: {0}")]
    Conflict(String),

    /// 429 Too Many Requests — rate limit exceeded.
    #[error("too many requests — please retry later")]
    TooManyRequests,

    /// 500 Internal Server Error — database or other unexpected failure.
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<surrealdb::Error> for AppError {
    fn from(e: surrealdb::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, AppError>;
