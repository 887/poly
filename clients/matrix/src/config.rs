//! Configuration helpers for the Matrix HTTP transport.
//!
//! This module is intentionally isolated inside `poly-matrix` so
//! Matrix-specific endpoint rules do not leak into the main app or shared
//! client contract.

use poly_client::{AuthCredentials, ClientError, ClientResult};

/// Default homeserver used when no custom URL is provided.
pub const DEFAULT_HOMESERVER_URL: &str = "https://matrix.org";

/// Well-known auto-discovery path.
pub const WELL_KNOWN_PATH: &str = "/.well-known/matrix/client";

/// Errors that can occur when building Matrix connection configuration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MatrixConfigError {
    /// The configured homeserver URL was empty or contained only slashes.
    #[error("homeserver URL cannot be empty")]
    EmptyUrl,

    /// Only HTTP and HTTPS endpoints are supported.
    #[error("homeserver URL must start with http:// or https://: {0}")]
    UnsupportedScheme(String),
}

/// Normalized connection configuration for one Matrix homeserver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixConfig {
    /// Homeserver base URL, no trailing slash.
    homeserver_url: String,
}

impl MatrixConfig {
    /// Create configuration for the default homeserver (matrix.org).
    #[must_use]
    pub fn default_homeserver() -> Self {
        Self {
            homeserver_url: DEFAULT_HOMESERVER_URL.to_string(),
        }
    }

    /// Create configuration for a custom homeserver.
    pub fn new(homeserver_url: impl Into<String>) -> Result<Self, MatrixConfigError> {
        let normalized = normalize_homeserver_url(homeserver_url.into())?;
        Ok(Self {
            homeserver_url: normalized,
        })
    }

    /// Normalized homeserver base URL with no trailing slash.
    #[must_use]
    pub fn homeserver_url(&self) -> &str {
        &self.homeserver_url
    }

    /// Build a full client-server API URL from a path.
    ///
    /// `path` should start with `/` and include the API version prefix,
    /// e.g. `/_matrix/client/v3/login`.
    #[must_use]
    pub fn api_url(&self, path: &str) -> String {
        format!("{}{path}", self.homeserver_url)
    }

    /// Stable identifier for this homeserver instance, used for routing
    /// and deduplication in multi-account scenarios.
    #[must_use]
    pub fn instance_id(&self) -> String {
        self.homeserver_url.clone()
    }
}

/// Authentication input parsed from generic `AuthCredentials`.
#[derive(Debug, Clone)]
pub enum MatrixAuthInput {
    /// Pre-existing access token (e.g. from saved session).
    AccessToken(String),
    /// Username + password login.
    UsernamePassword { username: String, password: String },
}

impl TryFrom<AuthCredentials> for MatrixAuthInput {
    type Error = ClientError;

    fn try_from(creds: AuthCredentials) -> ClientResult<Self> {
        match creds {
            AuthCredentials::Token(token) => Ok(Self::AccessToken(token)),
            AuthCredentials::EmailPassword { email, password } => {
                Ok(Self::UsernamePassword {
                    username: email,
                    password,
                })
            }
            _ => Err(ClientError::AuthFailed(
                "Matrix only supports token or username/password authentication".into(),
            )),
        }
    }
}

/// Normalize a homeserver URL: trim whitespace, strip trailing slashes,
/// validate scheme.
fn normalize_homeserver_url(raw: String) -> Result<String, MatrixConfigError> {
    let trimmed = raw.trim().trim_end_matches('/').to_string();

    if trimmed.is_empty() {
        return Err(MatrixConfigError::EmptyUrl);
    }

    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err(MatrixConfigError::UnsupportedScheme(trimmed));
    }

    Ok(trimmed)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn default_homeserver_is_matrix_org() {
        let config = MatrixConfig::default_homeserver();
        assert_eq!(config.homeserver_url(), "https://matrix.org");
    }

    #[test]
    fn custom_homeserver_strips_trailing_slash() {
        let config = MatrixConfig::new("https://my.server.tld/").unwrap();
        assert_eq!(config.homeserver_url(), "https://my.server.tld");
    }

    #[test]
    fn api_url_builds_correctly() {
        let config = MatrixConfig::default_homeserver();
        assert_eq!(
            config.api_url("/_matrix/client/v3/login"),
            "https://matrix.org/_matrix/client/v3/login"
        );
    }

    #[test]
    fn empty_url_rejected() {
        assert!(MatrixConfig::new("").is_err());
        assert!(MatrixConfig::new("   ").is_err());
    }

    #[test]
    fn unsupported_scheme_rejected() {
        assert!(MatrixConfig::new("ftp://matrix.org").is_err());
    }

    #[test]
    fn http_scheme_accepted() {
        let config = MatrixConfig::new("http://localhost:8008").unwrap();
        assert_eq!(config.homeserver_url(), "http://localhost:8008");
    }

    #[test]
    fn auth_input_from_token() {
        let input = MatrixAuthInput::try_from(AuthCredentials::Token("tok".into())).unwrap();
        assert!(matches!(input, MatrixAuthInput::AccessToken(t) if t == "tok"));
    }

    #[test]
    fn auth_input_from_email_password() {
        let input = MatrixAuthInput::try_from(AuthCredentials::EmailPassword {
            email: "@alice:matrix.org".into(),
            password: "secret".into(),
        })
        .unwrap();
        assert!(matches!(input, MatrixAuthInput::UsernamePassword { username, .. } if username == "@alice:matrix.org"));
    }
}
