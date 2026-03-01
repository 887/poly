//! Admin web UI for the backup server.
//!
//! TODO(phase-2.8.10): Implement Dioxus fullstack admin UI with:
//! - Connected accounts view
//! - Active sessions view
//! - Server configuration

/// Placeholder for admin web UI.
pub fn admin_routes() -> axum::Router {
    axum::Router::new().route("/admin", axum::routing::get(admin_page))
}

async fn admin_page() -> axum::response::Html<String> {
    axum::response::Html(
        r#"<!DOCTYPE html>
<html>
<head><title>Poly Backup Server — Admin</title></head>
<body>
    <h1>Poly Backup Server</h1>
    <p>Admin UI coming soon...</p>
</body>
</html>"#
            .to_string(),
    )
}
