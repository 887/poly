//! Matrix signup and test account helpers.

use poly_client::{AuthCredentials, ClientBackend as _, SignupCompleted};
use crate::MatrixClient;

/// Authenticate against a Matrix homeserver. Public so test panels can call it.
pub async fn authenticate(
    base_url: String,
    username: String,
    password: String,
) -> Result<SignupCompleted, String> {
    let mut backend = MatrixClient::with_homeserver(base_url).map_err(|e| e.to_string())?;
    let session = backend
        .authenticate(AuthCredentials::EmailPassword {
            email: username,
            password,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(SignupCompleted::new(session, Box::new(backend)))
}

fn owl_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate(u, e, p).await })
}

fn axolotl_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate(u, e, p).await })
}

/// Test accounts for the Matrix local dev server (port 9100).
pub fn get_test_accounts() -> &'static [poly_client::TestAccountEntry] {
    use poly_client::TestAccountEntry;
    const ACCOUNTS: &[TestAccountEntry] = &[
        TestAccountEntry {
            icon: "🦉",
            label: "Owl",
            server_label: "Matrix — localhost:9100",
            base_url: "http://localhost:9100",
            username: "owl",
            password: "testpass123",
            backend_slug: "matrix",
            authenticate: owl_auth,
        },
        TestAccountEntry {
            icon: "🦎",
            label: "Axolotl",
            server_label: "Matrix — localhost:9100",
            base_url: "http://localhost:9100",
            username: "axolotl",
            password: "testpass123",
            backend_slug: "matrix",
            authenticate: axolotl_auth,
        },
    ];
    ACCOUNTS
}
