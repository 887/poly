/// Server configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// Address to bind the HTTP server on (e.g. `0.0.0.0:7080`).
    pub bind_addr: String,
    /// Path to the SQLite database file.
    pub db_path: String,
    /// SurrealDB WebSocket endpoint (e.g. `ws://localhost:8000`).
    pub surreal_url: String,
    /// SurrealDB username (for network connection).
    pub surreal_user: String,
    /// SurrealDB password (for network connection).
    pub surreal_pass: String,
    /// Human-readable server name shown in `/server-info`.
    pub server_name: String,
    /// If true, signup requires an invite code.
    pub invite_only: bool,
    /// Secret used to sign JWTs. Set to a long random string in production.
    pub jwt_secret: String,
    /// JWT expiry in seconds (default: 30 days).
    pub jwt_expiry_secs: u64,
    /// Directory where uploaded files are stored on disk.
    pub uploads_dir: String,
}

impl Config {
    /// Load configuration from environment variables, falling back to sensible
    /// development defaults.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:7080".to_owned()),
            db_path: std::env::var("DB_PATH").unwrap_or_else(|_| "poly-server.db".to_owned()),
            surreal_url: std::env::var("SURREAL_URL")
                .unwrap_or_else(|_| "ws://localhost:8000".to_owned()),
            surreal_user: std::env::var("SURREAL_USER")
                .unwrap_or_else(|_| "root".to_owned()),
            surreal_pass: std::env::var("SURREAL_PASS")
                .unwrap_or_else(|_| "root".to_owned()),
            server_name: std::env::var("SERVER_NAME")
                .unwrap_or_else(|_| "My Poly Server".to_owned()),
            invite_only: std::env::var("INVITE_ONLY")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            jwt_secret: std::env::var("JWT_SECRET").unwrap_or_else(|_| {
                "change-me-in-production-please-use-a-long-random-string".to_owned()
            }),
            jwt_expiry_secs: std::env::var("JWT_EXPIRY_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60 * 60 * 24 * 30), // 30 days
            uploads_dir: std::env::var("POLY_SERVER_UPLOADS_DIR")
                .unwrap_or_else(|_| "./data/uploads".to_owned()),
        }
    }
}
