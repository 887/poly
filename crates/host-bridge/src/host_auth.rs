//! # `host_auth` — caller identity for the `/host/*` capability surface
//!
//! Phase A of `docs/plans/plan-host-substrate-capability-gating.md`.
//!
//! ## The threat this module defends against
//!
//! Every Poly shell binds the full `/host/*` route set on a **fixed
//! loopback port** (`apps/web` 3000, `apps/desktop-electron` 3001,
//! `apps/desktop` 3002, the standalone `poly-host` daemon 9333). Those
//! routes are the WASM bundle's syscall surface: subprocess spawn
//! (`/host/exec`), an outbound HTTP proxy (`/host/http`), and the KV store
//! that holds OAuth access/refresh tokens (`/host/kv/*`).
//!
//! A loopback port is **not** a trust boundary in a browser. Any page the
//! user has open in any tab can `fetch('http://127.0.0.1:3000/host/exec',
//! …)`. Before this module the router also carried
//! `CorsLayer::allow_origin(Any)`, so the attacker page could read the
//! reply as well as send the request. Concretely, the pre-Phase-A holes:
//!
//! | Attacker | Capability |
//! |---|---|
//! | any web page in the user's browser | arbitrary program execution via `POST /host/exec` |
//! | any web page in the user's browser | read every OAuth token via `POST /host/kv/get {"key":"account_tokens"}` |
//! | any *installed plugin* | read/clobber any other plugin's KV namespace (the `plugin` field was caller-supplied) |
//!
//! ### What is in scope
//!
//! * **Cross-origin browser callers.** A hostile page must not be able to
//!   reach a `/host/*` route, and must not be able to *learn* the token.
//! * **Plugin-vs-plugin confusion.** A plugin holding its own derived
//!   token must not be able to impersonate the shell or another plugin.
//!
//! ### What is explicitly *out* of scope
//!
//! * **A hostile native process running as the same OS user.** It can read
//!   the shell's memory, the SQLite file and any key material we hold, so
//!   no in-process token helps. Poly's confinement story for that
//!   adversary is the OS user account, not this module.
//! * **A compromised shell bundle.** The shell *is* the trusted principal
//!   here; `CallerId::Shell` is full authority over the app's own data.
//!
//! ## The mechanism
//!
//! 1. **Mint.** [`HostAuth::mint`] draws 32 bytes from the OS CSPRNG at
//!    shell start and hex-encodes them. The token lives only in the
//!    shell process's memory — it is never written to `poly_kv`, never
//!    logged, and never persisted, so it changes on every restart. That
//!    is deliberate: a stale token on disk is a stale token an attacker
//!    can steal, and re-bootstrapping is one cheap same-origin GET.
//!
//! 2. **Present.** Every `/host/*` request carries
//!    `Authorization: Bearer <token>`. Plugins additionally send
//!    [`PLUGIN_HEADER`] naming themselves, and present the *derived*
//!    token from [`HostAuth::derive_plugin_token`] rather than the master
//!    one. The derivation is `HMAC-SHA256(master, "poly-host-plugin-v1:"
//!    || plugin_id)`, so holding a plugin token yields neither the master
//!    token nor any sibling's token.
//!
//! 3. **Learn.** The client bootstraps via `GET` [`ROUTE_SESSION`], which
//!    is the one route exempt from bearer auth. The server gates it on
//!    request *provenance* instead: the `Origin` header, when present,
//!    must be one of the shell's own loopback origins, and the browser-set
//!    `Sec-Fetch-Site` header (which page script cannot forge — it is a
//!    forbidden header name) must say `same-origin` or `same-site`. A page
//!    on `https://evil.test` therefore gets a 403 from the mint route
//!    *and* a CORS rejection on the reply, so it never sees the token and
//!    cannot reach any other `/host/*` route.
//!
//!    **DNS rebinding is the one case those two signals miss**: a page
//!    served from `http://evil.test:3000` whose DNS is rebound to
//!    127.0.0.1 is same-origin *with itself*, so it sends no `Origin` and
//!    `Sec-Fetch-Site: same-origin`. The shell therefore also requires the
//!    `Host` header to be a loopback name, which a rebound request cannot
//!    produce (`Host: evil.test:3000`). That check lives in
//!    `apps/poly-host`'s `require_host_auth` because it needs the router's
//!    origin configuration, and it covers every `/host/*` route including
//!    the mint route.
//!
//! 4. **Rotate.** Because the token is per-process, a shell restart
//!    invalidates every cached client token. [`send_authorized`] handles
//!    that transparently: a `401` invalidates the cache entry, re-fetches
//!    from [`ROUTE_SESSION`], and replays the request exactly once.
//!
//! ## Fail-closed, with one named escape hatch
//!
//! Enforcement is **on by default**. Setting
//! [`AUTH_DISABLE_ENV`]`=1` turns verification into a
//! no-op that reports every caller as [`CallerId::Shell`]. It exists so a
//! shell whose client half has not yet been taught to send the header
//! stays runnable during the rollout; it is logged loudly at startup and
//! must never be set in a shipped build.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// `GET` route that mints/returns the calling shell's session token.
///
/// Exempt from bearer verification (it is the bootstrap), gated on request
/// provenance instead — see the module docs.
pub const ROUTE_SESSION: &str = "/host/session";

/// Header a plugin caller uses to name itself. Meaningless without the
/// matching derived bearer token.
pub const PLUGIN_HEADER: &str = "x-poly-plugin";

/// Environment variable that disables `/host/*` bearer enforcement.
///
/// Deliberately ugly. Set to `1` only while a shell's client half is being
/// taught to send the token; never in a shipped build.
pub const AUTH_DISABLE_ENV: &str = "POLY_HOST_AUTH_INSECURE_DISABLE";

/// Body of `GET` [`ROUTE_SESSION`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostSessionResponse {
    /// `true` when a token was issued.
    pub ok: bool,
    /// The bearer token to present on every `/host/*` request.
    #[serde(default)]
    pub token: String,
    /// Populated instead of `token` when the mint route refuses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// Who is on the other end of a `/host/*` request.
///
/// Established by [`HostAuth::verify`] from the presented bearer token —
/// **never** from a field in the request body. Phase C derives KV
/// namespaces from this value precisely so a caller cannot name someone
/// else's keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallerId {
    /// The app's own WASM bundle. Full authority over the app's own rows
    /// (`app_settings`, `account_tokens`, plugin admin), but still barred
    /// from the host-internal namespace.
    Shell,
    /// An installed plugin, identified by its registry id.
    Plugin {
        /// Registry id of the plugin (e.g. `"discord"`).
        id: String,
    },
}

impl CallerId {
    /// KV key prefix this caller is confined to, or `None` when the caller
    /// may name app-level keys directly (the shell).
    #[must_use]
    pub fn kv_prefix(&self) -> Option<String> {
        match *self {
            Self::Shell => None,
            Self::Plugin { ref id } => Some(format!("plugin:{id}:")),
        }
    }

    /// Stable, log-safe description of the caller.
    #[must_use]
    pub fn label(&self) -> String {
        match *self {
            Self::Shell => "shell".to_string(),
            Self::Plugin { ref id } => format!("plugin:{id}"),
        }
    }
}

/// Why a `/host/*` request was refused at the identity layer.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum HostAuthError {
    /// No `Authorization` header at all.
    #[error("missing Authorization header on a /host/* request")]
    Missing,
    /// Present but not `Bearer <token>`.
    #[error("malformed Authorization header (expected `Bearer <token>`)")]
    Malformed,
    /// Correct shape, wrong secret.
    #[error("bearer token does not match this shell's session token")]
    Mismatch,
    /// The `x-poly-plugin` header named a plugin whose derived token was
    /// not the one presented.
    #[error("caller claimed plugin `{0}` but did not present that plugin's derived token")]
    PluginMismatch(String),
    /// The OS CSPRNG refused. Non-recoverable: refuse to start rather than
    /// fall back to a predictable token.
    #[error("could not draw entropy for the host session token: {0}")]
    Entropy(String),
}

// ─── Server side ─────────────────────────────────────────────────────────────
//
// Native-only: the verifier runs inside the shell's axum server. The WASM
// bundle is a *client* of this surface and never verifies anything, so
// gating the whole verifier keeps hmac/sha2/getrandom out of the WASM
// bundle.

/// The shell's session token plus the enforcement switch.
///
/// Construct once per shell process with [`HostAuth::mint`]; clone the
/// resulting [`HostState`](../../../apps/poly-host/src/lib.rs) freely.
///
/// See the module documentation for the threat model this implements.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct HostAuth {
    token: String,
    enforced: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Debug for HostAuth {
    /// Redacts the token — it must never reach a log line.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostAuth")
            .field("token", &"<redacted>")
            .field("enforced", &self.enforced)
            .finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl HostAuth {
    /// Domain-separation prefix for per-plugin token derivation.
    const PLUGIN_KDF_LABEL: &'static [u8] = b"poly-host-plugin-v1:";

    /// Mint a fresh 32-byte session token from the OS CSPRNG.
    ///
    /// Enforcement follows [`AUTH_DISABLE_ENV`]: on unless it is set to
    /// `1`.
    ///
    /// # Errors
    ///
    /// [`HostAuthError::Entropy`] if the OS random source is unavailable.
    /// Callers must propagate — a predictable token is worse than no
    /// server.
    pub fn mint() -> Result<Self, HostAuthError> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| HostAuthError::Entropy(e.to_string()))?;
        Ok(Self {
            token: hex::encode(bytes),
            enforced: !Self::disabled_by_env(),
        })
    }

    /// Build a verifier around a caller-supplied token — the in-memory
    /// constructor tests use so they do not depend on the OS CSPRNG or on
    /// process environment.
    #[must_use]
    pub fn with_token(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            enforced: true,
        }
    }

    /// Same as [`with_token`](Self::with_token) but with enforcement off —
    /// used to test the escape hatch's behaviour without mutating the
    /// process environment.
    #[must_use]
    pub fn unenforced(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            enforced: false,
        }
    }

    fn disabled_by_env() -> bool {
        std::env::var(AUTH_DISABLE_ENV).is_ok_and(|v| v == "1")
    }

    /// The shell's master token. Handed out only by the provenance-gated
    /// [`ROUTE_SESSION`] route.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Whether verification actually rejects. `false` only under
    /// [`AUTH_DISABLE_ENV`].
    #[must_use]
    pub const fn enforced(&self) -> bool {
        self.enforced
    }

    /// Per-plugin token: `HMAC-SHA256(master, "poly-host-plugin-v1:" || id)`,
    /// hex-encoded.
    ///
    /// Handing a plugin this value grants it exactly its own namespace: it
    /// cannot invert the HMAC to recover the master token, and it cannot
    /// compute a sibling's token without the master.
    #[must_use]
    pub fn derive_plugin_token(&self, plugin_id: &str) -> String {
        use hmac::{KeyInit as _, Mac as _};
        type HmacSha256 = hmac::Hmac<sha2::Sha256>;

        // `new_from_slice` only fails for key sizes HMAC cannot pad, which
        // is impossible for HMAC (any length is valid); the fallback keeps
        // the function total without an unwrap.
        let Ok(mut mac) = HmacSha256::new_from_slice(self.token.as_bytes()) else {
            return String::new();
        };
        mac.update(Self::PLUGIN_KDF_LABEL);
        mac.update(plugin_id.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Establish the caller's identity from the presented headers.
    ///
    /// `authorization` is the raw header value (`"Bearer <token>"`);
    /// `plugin` is the raw [`PLUGIN_HEADER`] value when the caller claims
    /// to be a plugin.
    ///
    /// Deviation from the plan's `verify(&self, header)` signature: the
    /// plugin claim needs a second input, because the derived token is a
    /// one-way function of the id and cannot be reversed back into one.
    /// The claim is still *verified*, never trusted.
    ///
    /// # Errors
    ///
    /// See [`HostAuthError`]. With enforcement disabled every caller
    /// resolves to [`CallerId::Shell`].
    pub fn verify(
        &self,
        authorization: Option<&str>,
        plugin: Option<&str>,
    ) -> Result<CallerId, HostAuthError> {
        if !self.enforced {
            return Ok(CallerId::Shell);
        }
        let raw = authorization.ok_or(HostAuthError::Missing)?;
        let presented = raw
            .strip_prefix("Bearer ")
            .or_else(|| raw.strip_prefix("bearer "))
            .ok_or(HostAuthError::Malformed)?
            .trim();
        if presented.is_empty() {
            return Err(HostAuthError::Malformed);
        }
        match plugin {
            None => {
                if ct_eq(presented, &self.token) {
                    Ok(CallerId::Shell)
                } else {
                    Err(HostAuthError::Mismatch)
                }
            }
            Some(id) => {
                let id = id.trim();
                // A `:` in a plugin id would let `plugin:a:b` sit *inside*
                // `plugin:a`'s KV prefix, so one plugin could read
                // another's rows purely by being named cleverly. Reject at
                // the identity layer, which is the single choke point.
                if id.is_empty() || id.contains(':') {
                    return Err(HostAuthError::Malformed);
                }
                if ct_eq(presented, &self.derive_plugin_token(id)) {
                    Ok(CallerId::Plugin { id: id.to_string() })
                } else {
                    Err(HostAuthError::PluginMismatch(id.to_string()))
                }
            }
        }
    }
}

/// Constant-time string comparison — never leak the token byte-by-byte
/// through response timing.
#[cfg(not(target_arch = "wasm32"))]
fn ct_eq(a: &str, b: &str) -> bool {
    use subtle::ConstantTimeEq as _;
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

// ─── Client side ─────────────────────────────────────────────────────────────
//
// Compiles on every target, including wasm32: this is the half the WASM
// bundle uses to learn and present the token.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn token_cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Install a known token for `base_url` without a round trip.
///
/// Used by native callers that share a process with the shell (they can
/// read [`HostAuth::token`] directly) and by tests.
pub fn seed_client_token(base_url: &str, token: impl Into<String>) {
    if let Ok(mut cache) = token_cache().lock() {
        let _prev = cache.insert(base_url.to_string(), token.into());
    }
}

/// Drop the cached token for `base_url`. Called automatically on a `401`
/// so a shell restart re-bootstraps instead of failing forever.
pub fn forget_client_token(base_url: &str) {
    if let Ok(mut cache) = token_cache().lock() {
        let _prev = cache.remove(base_url);
    }
}

fn cached_token(base_url: &str) -> Option<String> {
    token_cache().lock().ok()?.get(base_url).cloned()
}

/// Return the token for `base_url`, fetching it from [`ROUTE_SESSION`] on
/// a cache miss.
///
/// Returns `None` when the shell refuses to mint (e.g. the request was not
/// same-origin) — callers then send the request unauthenticated and let
/// the server produce the 401.
pub async fn client_token(http: &reqwest::Client, base_url: &str) -> Option<String> {
    if let Some(tok) = cached_token(base_url) {
        return Some(tok);
    }
    let url = format!("{base_url}{ROUTE_SESSION}");
    let resp = http.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: HostSessionResponse = resp.json().await.ok()?;
    if !body.ok || body.token.is_empty() {
        return None;
    }
    seed_client_token(base_url, body.token.clone());
    Some(body.token)
}

/// Send `builder` with the shell session token attached, re-bootstrapping
/// and replaying exactly once if the shell answers `401`.
///
/// This is the single choke point every host-bridge client uses, so the
/// bootstrap/rotation policy lives in one place rather than at each call
/// site.
///
/// # Errors
///
/// Propagates the underlying `reqwest` transport error.
pub async fn send_authorized(
    http: &reqwest::Client,
    base_url: &str,
    builder: reqwest::RequestBuilder,
) -> Result<reqwest::Response, reqwest::Error> {
    let replay = builder.try_clone();
    let first = match client_token(http, base_url).await {
        Some(tok) => builder.header("authorization", format!("Bearer {tok}")),
        None => builder,
    };
    let resp = first.send().await?;
    if resp.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(resp);
    }
    let Some(replay) = replay else {
        return Ok(resp);
    };
    forget_client_token(base_url);
    match client_token(http, base_url).await {
        Some(tok) => {
            replay
                .header("authorization", format!("Bearer {tok}"))
                .send()
                .await
        }
        None => Ok(resp),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn shell_token_accepted_and_wrong_token_rejected() {
        let auth = HostAuth::with_token("s3cret");
        assert_eq!(
            auth.verify(Some("Bearer s3cret"), None).unwrap(),
            CallerId::Shell
        );
        assert_eq!(
            auth.verify(Some("Bearer nope"), None).unwrap_err(),
            HostAuthError::Mismatch
        );
        assert_eq!(auth.verify(None, None).unwrap_err(), HostAuthError::Missing);
        assert_eq!(
            auth.verify(Some("s3cret"), None).unwrap_err(),
            HostAuthError::Malformed
        );
        assert_eq!(
            auth.verify(Some("Bearer   "), None).unwrap_err(),
            HostAuthError::Malformed
        );
    }

    #[test]
    fn plugin_token_is_scoped_to_its_own_id() {
        let auth = HostAuth::with_token("s3cret");
        let a = auth.derive_plugin_token("plugin-a");
        let b = auth.derive_plugin_token("plugin-b");
        assert_ne!(a, b);
        assert_ne!(a, auth.token());

        assert_eq!(
            auth.verify(Some(&format!("Bearer {a}")), Some("plugin-a"))
                .unwrap(),
            CallerId::Plugin {
                id: "plugin-a".to_string()
            }
        );
        // plugin-a's token does not let it claim to be plugin-b …
        assert_eq!(
            auth.verify(Some(&format!("Bearer {a}")), Some("plugin-b"))
                .unwrap_err(),
            HostAuthError::PluginMismatch("plugin-b".to_string())
        );
        // … nor to claim shell authority.
        assert_eq!(
            auth.verify(Some(&format!("Bearer {a}")), None).unwrap_err(),
            HostAuthError::Mismatch
        );
    }

    #[test]
    fn master_token_cannot_be_used_to_claim_a_plugin_identity() {
        let auth = HostAuth::with_token("s3cret");
        assert_eq!(
            auth.verify(Some("Bearer s3cret"), Some("plugin-a"))
                .unwrap_err(),
            HostAuthError::PluginMismatch("plugin-a".to_string())
        );
    }

    #[test]
    fn plugin_ids_containing_a_colon_are_rejected() {
        let auth = HostAuth::with_token("s3cret");
        // Even presenting the *correct* derived token for the colon-bearing
        // id does not get it past the identity layer.
        let tok = auth.derive_plugin_token("a:b");
        assert_eq!(
            auth.verify(Some(&format!("Bearer {tok}")), Some("a:b"))
                .unwrap_err(),
            HostAuthError::Malformed
        );
    }

    #[test]
    fn kv_prefix_is_derived_from_identity_not_input() {
        assert_eq!(CallerId::Shell.kv_prefix(), None);
        assert_eq!(
            CallerId::Plugin {
                id: "discord".to_string()
            }
            .kv_prefix(),
            Some("plugin:discord:".to_string())
        );
    }

    #[test]
    fn minted_tokens_are_unique_and_hex() {
        let a = HostAuth::mint().unwrap();
        let b = HostAuth::mint().unwrap();
        assert_ne!(a.token(), b.token());
        assert_eq!(a.token().len(), 64);
        assert!(a.token().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn debug_never_prints_the_token() {
        let auth = HostAuth::with_token("super-secret-value");
        let rendered = format!("{auth:?}");
        assert!(!rendered.contains("super-secret-value"), "{rendered}");
    }

    #[test]
    fn unenforced_accepts_anything_as_shell() {
        let auth = HostAuth::unenforced("s3cret");
        assert!(!auth.enforced());
        assert_eq!(auth.verify(None, None).unwrap(), CallerId::Shell);
        assert_eq!(
            auth.verify(Some("Bearer garbage"), Some("x")).unwrap(),
            CallerId::Shell
        );
    }

    #[test]
    fn client_token_cache_round_trips() {
        let base = "http://127.0.0.1:65000";
        seed_client_token(base, "tok-1");
        assert_eq!(cached_token(base).as_deref(), Some("tok-1"));
        forget_client_token(base);
        assert_eq!(cached_token(base), None);
    }
}
