//! Configuration helpers for the Stoat HTTP/WebSocket transport.
//!
//! This module is intentionally isolated inside `poly-stoat` so Stoat-specific
//! endpoint rules do not leak into the main app or shared client contract.

use poly_client::{AuthCredentials, ClientError, ClientResult};

/// Official Stoat API root used by the default client constructor.
pub const OFFICIAL_STOAT_BASE_URL: &str = "https://api.stoat.chat";

/// Errors that can occur when building Stoat connection configuration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoatConfigError {
    /// The configured base URL was empty or contained only slashes.
    #[error("Stoat base URL cannot be empty")]
    EmptyBaseUrl,

    /// Only HTTP and HTTPS endpoints are supported.
    #[error("Stoat base URL must start with http:// or https://: {0}")]
    UnsupportedScheme(String),
}

/// Normalized connection configuration for one Stoat instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoatConfig {
    base_url: String,
}

impl StoatConfig {
    /// Create configuration for the official Stoat instance.
    #[must_use]
    pub fn official() -> Self {
        Self {
            base_url: OFFICIAL_STOAT_BASE_URL.to_string(),
        }
    }

    /// Create configuration for a custom Stoat instance.
    pub fn new(base_url: impl Into<String>) -> Result<Self, StoatConfigError> {
        let base_url_string: String = base_url.into();
        let normalized = normalize_base_url(&base_url_string)?;
        Ok(Self {
            base_url: normalized,
        })
    }

    /// Normalized REST API base URL with no trailing slash.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Build a REST URL relative to the configured API root.
    #[must_use]
    pub fn rest_url(&self, path: &str) -> String {
        let normalized_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        format!("{}{}", self.base_url, normalized_path)
    }

    /// Derive the Bonfire websocket URL from the REST API root.
    #[must_use]
    pub fn websocket_url(&self) -> String {
        let ws_root = if let Some(rest) = self.base_url.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = self.base_url.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            self.base_url.clone()
        };
        format!("{ws_root}/ws")
    }

    /// Stable instance identifier for routing/session persistence.
    #[must_use]
    pub fn instance_id(&self) -> String {
        self.base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .replace('/', "~")
    }
}

/// Authentication input understood by the Stoat backend transport layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoatAuthInput {
    /// Resume an existing session using a previously issued token.
    SessionToken(String),
    /// Perform email/password login against `POST /auth/session/login`.
    EmailPassword { email: String, password: String },
}

impl TryFrom<AuthCredentials> for StoatAuthInput {
    type Error = ClientError;

    fn try_from(value: AuthCredentials) -> ClientResult<Self> {
        match value {
            AuthCredentials::Token(token) => Ok(Self::SessionToken(token)),
            AuthCredentials::EmailPassword { email, password } => {
                Ok(Self::EmailPassword { email, password })
            }
            AuthCredentials::OAuth { .. }
            | AuthCredentials::DeviceCode { .. }
            | AuthCredentials::PolyServer { .. } => Err(ClientError::AuthFailed(
                "Stoat only supports token or email/password credentials".to_string(),
            )),
        }
    }
}

fn normalize_base_url(base_url: &str) -> Result<String, StoatConfigError> {
    let trimmed = base_url.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err(StoatConfigError::EmptyBaseUrl);
    }
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err(StoatConfigError::UnsupportedScheme(trimmed));
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::{OFFICIAL_STOAT_BASE_URL, StoatAuthInput, StoatConfig, StoatConfigError};
    use poly_client::{AuthCredentials, ClientError};

    #[test]
    fn official_config_uses_documented_base_url() {
        assert_eq!(StoatConfig::official().base_url(), OFFICIAL_STOAT_BASE_URL);
    }

    #[test]
    fn custom_config_trims_trailing_slashes() {
        assert_eq!(
            StoatConfig::new("https://chat.example.test///")
                .map(|config| config.base_url().to_string()),
            Ok("https://chat.example.test".to_string())
        );
    }

    #[test]
    fn custom_config_rejects_unsupported_scheme() {
        assert_eq!(
            StoatConfig::new("ftp://chat.example.test"),
            Err(StoatConfigError::UnsupportedScheme(
                "ftp://chat.example.test".to_string()
            ))
        );
    }

    #[test]
    fn websocket_url_tracks_http_scheme() {
        assert_eq!(
            StoatConfig::new("http://127.0.0.1:8080/api").map(|config| config.websocket_url()),
            Ok("ws://127.0.0.1:8080/api/ws".to_string())
        );
        assert_eq!(
            StoatConfig::new("https://api.stoat.chat").map(|config| config.websocket_url()),
            Ok("wss://api.stoat.chat/ws".to_string())
        );
    }

    #[test]
    fn instance_id_removes_scheme_and_sanitizes_path() {
        assert_eq!(
            StoatConfig::new("https://chat.example.test/custom/api")
                .map(|config| config.instance_id()),
            Ok("chat.example.test~custom~api".to_string())
        );
    }

    #[test]
    fn stoat_auth_input_accepts_email_password() {
        assert!(matches!(
            StoatAuthInput::try_from(AuthCredentials::EmailPassword {
                email: "alice@example.com".to_string(),
                password: "hunter2".to_string(),
            }),
            Ok(StoatAuthInput::EmailPassword {
                email,
                password,
            })
                if email == "alice@example.com" && password == "hunter2"
        ));
    }

    #[test]
    fn stoat_auth_input_rejects_unrelated_credentials() {
        assert!(matches!(
            StoatAuthInput::try_from(AuthCredentials::OAuth {
                token: "oauth-token".to_string(),
            }),
            Err(ClientError::AuthFailed(message))
                if message == "Stoat only supports token or email/password credentials"
        ));
    }
}
