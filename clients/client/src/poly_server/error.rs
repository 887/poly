//! Error types for the poly-server client.

use thiserror::Error;

/// Errors from poly-server client operations.
#[derive(Debug, Error)]
pub enum PolyServerError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Server returned an error response.
    #[error("Server error ({status}): {message}")]
    Server {
        /// HTTP status code.
        status: u16,
        /// Error message from the server.
        message: String,
    },

    /// Authentication failed.
    #[error("Authentication failed: {0}")]
    Auth(String),

    /// WebSocket error.
    #[error("WebSocket error: {0}")]
    WebSocket(String),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Ed25519 signature error.
    #[error("Signature error: {0}")]
    Signature(String),

    /// Hex encoding/decoding error.
    #[error("Hex error: {0}")]
    Hex(#[from] hex::FromHexError),

    /// The client is not authenticated.
    #[error("Not authenticated — call signup() or signin() first")]
    NotAuthenticated,
}

/// Result type alias for poly-server client operations.
pub type Result<T> = std::result::Result<T, PolyServerError>;
