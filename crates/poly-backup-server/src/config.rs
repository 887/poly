//! Server configuration from environment variables.

use std::net::SocketAddr;
use std::path::PathBuf;

/// Full server configuration loaded from environment variables at startup.
#[derive(Debug, Clone)]
pub struct Config {
    /// Server-wide access passphrase for new account registration.
    /// Clients must include this in the auth request.
    pub passphrase: String,
    /// Maximum number of registered accounts. 0 = unlimited.
    pub max_accounts: usize,
    /// Days of inactivity before a session token expires. Rolling — reset on every API call.
    pub token_expiry_days: u64,
    /// PoW difficulty in leading-zero bits for API auth challenges.
    pub pow_difficulty: u32,
    /// PoW difficulty for the admin login page (lower — human-interactive).
    pub admin_pow_difficulty: u32,
    /// Server bind address.
    pub bind: SocketAddr,
    /// Directory for SurrealKV database files.
    pub data_dir: PathBuf,
    /// Maximum failed auth attempts per IP before rate-limit kicks in.
    pub rate_limit_max: u32,
    /// Rate-limit sliding window in seconds.
    pub rate_limit_window_secs: u64,
    /// Admin UI username.
    pub admin_user: String,
    /// Admin UI password (stored in memory; compare via constant-time SHA-256 hash).
    pub admin_password: String,
    /// Admin session token expiry in hours.
    pub admin_session_hours: u64,
    /// Max admin login attempts per minute (global, across all IPs).
    pub admin_rate_limit_per_minute: u32,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// # Panics
    /// Panics if `POLY_BIND` is set to an invalid socket address, or if
    /// `POLY_PASSPHRASE` / `POLY_ADMIN_PASSWORD` are not set in production.
    pub fn from_env() -> Self {
        let bind_str = std::env::var("POLY_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
        let bind: SocketAddr = bind_str
            .parse()
            .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 8080)));

        Self {
            passphrase: std::env::var("POLY_PASSPHRASE").unwrap_or_else(|_| "changeme".to_string()),
            max_accounts: std::env::var("POLY_MAX_ACCOUNTS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            token_expiry_days: std::env::var("POLY_TOKEN_EXPIRY_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(365),
            pow_difficulty: std::env::var("POLY_POW_DIFFICULTY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(20),
            admin_pow_difficulty: std::env::var("POLY_ADMIN_POW_DIFFICULTY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(16),
            bind,
            data_dir: std::env::var("POLY_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./data")),
            rate_limit_max: std::env::var("POLY_RATE_LIMIT_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            rate_limit_window_secs: std::env::var("POLY_RATE_LIMIT_WINDOW_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600),
            admin_user: std::env::var("POLY_ADMIN_USER").unwrap_or_else(|_| "admin".to_string()),
            admin_password: std::env::var("POLY_ADMIN_PASSWORD")
                .unwrap_or_else(|_| "changeme".to_string()),
            admin_session_hours: std::env::var("POLY_ADMIN_SESSION_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(4),
            admin_rate_limit_per_minute: std::env::var("POLY_ADMIN_RATE_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        }
    }
}
