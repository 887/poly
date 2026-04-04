//! Simple opaque token auth for mock test servers.
//!
//! No JWT — just random tokens mapped to user IDs in a DashMap.

use dashmap::DashMap;
use uuid::Uuid;

/// Shared auth state: maps opaque tokens → user IDs.
#[derive(Clone, Default)]
pub struct AuthState {
    /// token → user_id
    tokens: DashMap<String, String>,
}

impl AuthState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new token for the given user ID. Returns the token string.
    pub fn create_token(&self, user_id: &str) -> String {
        let token = Uuid::new_v4().to_string();
        self.tokens.insert(token.clone(), user_id.to_string());
        token
    }

    /// Validate a token and return the associated user ID.
    pub fn validate(&self, token: &str) -> Option<String> {
        self.tokens.get(token).map(|entry| entry.value().clone())
    }

    /// Revoke a token (logout).
    pub fn revoke(&self, token: &str) {
        self.tokens.remove(token);
    }

    /// Clear all tokens (used by `/reset`).
    pub fn clear(&self) {
        self.tokens.clear();
    }
}

/// Trait for extracting auth from request headers.
/// Implement per-backend since header formats vary
/// (Matrix: `Bearer`, Stoat: `x-session-token`, Discord: `Bot`, etc.)
pub trait TokenAuth {
    /// Extract user ID from request headers, if authenticated.
    fn extract_user_id(&self, auth_header: Option<&str>) -> Option<String>;
}

impl TokenAuth for AuthState {
    fn extract_user_id(&self, auth_header: Option<&str>) -> Option<String> {
        let header = auth_header?;
        let token = header.strip_prefix("Bearer ").unwrap_or(header);
        self.validate(token)
    }
}
