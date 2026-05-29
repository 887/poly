//! # `route` — `HostRoute` trait and generic `call` helper
//!
//! Unifies the `POST → JSON → typed-error` pipeline that
//! `codec_opus_client`, `aead_client`, and `udp_client` previously
//! each re-implemented as a private `post_json` method.
//!
//! ## Design
//!
//! Every host-bridge sub-route is modelled as a zero-sized struct that
//! implements [`HostRoute`]:
//!
//! ```rust,ignore
//! pub struct AeadCreateRoute;
//!
//! impl HostRoute for AeadCreateRoute {
//!     type Req  = AeadCreateRequest;
//!     type Resp = AeadCreateResponse;
//!     type Err  = AeadClientError;
//!     fn endpoint() -> &'static str { ROUTE_AEAD_CREATE }
//! }
//! ```
//!
//! Then the typed client delegates to the shared [`call`] function:
//!
//! ```rust,ignore
//! let resp = call::<AeadCreateRoute>(&self.http, &self.base_url, req).await?;
//! ```
//!
//! ## Policy constants
//!
//! Each `HostRoute` impl may override [`HostRoute::RETRIES`] and
//! [`HostRoute::TIMEOUT_MS`] to encode per-endpoint retry / timeout policy.
//! The [`call`] function respects these constants — callers don't need to
//! know about them.
//!
//! Defaults: 0 retries, 30 000 ms timeout (native only — see
//! [wasm32 note] below).
//!
//! ## wasm32 note
//!
//! `tokio::time::timeout` panics on `wasm32-unknown-unknown` because
//! `Instant::now()` is not implemented. The timeout constant is therefore
//! only enforced on native targets; on wasm32 the browser's `fetch` timeout
//! applies instead and [`HostRoute::TIMEOUT_MS`] is silently ignored.
//!
//! ## Error bridging
//!
//! Each route's `Err` type must implement `From<TransportError>` so that
//! [`call`] can convert transport-level failures (reqwest, JSON, timeout)
//! without knowing the concrete error type. Per-client error enums add a
//! `#[from] TransportError` variant or implement `From` manually.

use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

// ── TransportError ─────────────────────────────────────────────────────────────

/// Transport-level failure in the POST → JSON → typed-response pipeline.
///
/// This is the *lowest-common-denominator* error type that [`call`] can
/// produce regardless of which route is being called. Per-route error types
/// implement `From<TransportError>` so [`call`]'s `?` can coerce cleanly.
#[derive(Debug, Error)]
pub enum TransportError {
    /// `reqwest` HTTP error (connect, TLS, read, …).
    #[error("host-bridge transport: {0}")]
    Http(#[from] reqwest::Error),
    /// The response body was not valid JSON.
    #[error("host-bridge JSON parse: {0}")]
    Json(#[from] serde_json::Error),
    /// The request exceeded the per-route timeout (native only).
    #[error("host-bridge timeout after {ms}ms")]
    Timeout {
        /// The configured timeout in milliseconds ([`HostRoute::TIMEOUT_MS`]).
        ms: u64,
    },
}

// ── HostRoute ──────────────────────────────────────────────────────────────────

/// A single host-bridge sub-route: one endpoint, one request type, one
/// response type, one error type, and optional per-route policy.
///
/// Implement this trait on a zero-sized marker struct, then call the
/// generic [`call`] helper instead of duplicating the POST pipeline.
///
/// # Example
///
/// ```rust,ignore
/// pub struct AeadCreateRoute;
///
/// impl HostRoute for AeadCreateRoute {
///     type Req  = AeadCreateRequest;
///     type Resp = AeadCreateResponse;
///     type Err  = AeadClientError;
///     fn endpoint() -> &'static str { ROUTE_AEAD_CREATE }
/// }
/// ```
pub trait HostRoute {
    /// Serialisable request body.
    type Req: Serialize;
    /// Deserialisable response body.
    type Resp: DeserializeOwned;
    /// Typed error returned to callers. Must be constructible from a
    /// [`TransportError`] so [`call`] can propagate HTTP / JSON / timeout
    /// failures without knowing the concrete type.
    type Err: From<TransportError>;

    /// Absolute HTTP path of this endpoint, e.g. `"/host/aead/create"`.
    fn endpoint() -> &'static str;

    /// Number of automatic retries on transport error (default: 0).
    ///
    /// The retry loop only retries on [`TransportError::Http`] (network
    /// failures); JSON parse errors and server-side `ok: false` responses
    /// are not retried.
    const RETRIES: usize = 0;

    /// Per-request timeout in milliseconds (default: 30 000 ms = 30 s).
    ///
    /// Enforced on native targets via `tokio::time::timeout`. Silently
    /// ignored on `wasm32-unknown-unknown` — browser fetch timeout applies.
    const TIMEOUT_MS: u64 = 30_000;
}

// ── call ───────────────────────────────────────────────────────────────────────

/// Send one POST request for route `R` and decode the typed response.
///
/// Constructs the full URL as `base_url + R::endpoint()`, serialises
/// `req` as JSON, and deserialises the response text as `R::Resp`.
///
/// Respects [`HostRoute::RETRIES`] and [`HostRoute::TIMEOUT_MS`] (native
/// only for the timeout).
///
/// # Errors
///
/// Returns `R::Err` on any transport failure. Callers handle server-side
/// errors by inspecting `R::Resp` fields (e.g. `resp.ok == false`).
pub async fn call<R: HostRoute>(
    http: &reqwest::Client,
    base_url: &str,
    req: R::Req,
) -> Result<R::Resp, R::Err>
where
    R::Req: Send + Sync,
    R::Resp: Send,
    R::Err: Send,
{
    let url = format!("{}{}", base_url, R::endpoint());

    // attempt 0 … RETRIES (inclusive).
    let max_attempts = R::RETRIES.saturating_add(1);
    let mut i = 0usize;
    loop {
        match do_post::<R>(http, &url, &req).await {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                i = i.saturating_add(1);
                if i >= max_attempts {
                    return Err(e);
                }
                // continue to retry
            }
        }
    }
}

// ── native: enforce TIMEOUT_MS via tokio::time::timeout ───────────────────────

#[cfg(not(target_arch = "wasm32"))]
async fn do_post<R: HostRoute>(
    http: &reqwest::Client,
    url: &str,
    req: &R::Req,
) -> Result<R::Resp, R::Err>
where
    R::Req: Sync,
    R::Resp: Send,
    R::Err: Send,
{
    use std::time::Duration;

    let fut = post_and_parse::<R::Resp>(http, url, req);
    match tokio::time::timeout(Duration::from_millis(R::TIMEOUT_MS), fut).await {
        Ok(result) => result.map_err(|e: TransportError| R::Err::from(e)),
        Err(_elapsed) => {
            Err(R::Err::from(TransportError::Timeout { ms: R::TIMEOUT_MS }))
        }
    }
}

// ── wasm32: skip Instant-based timeout; browser fetch timeout applies ─────────

#[cfg(target_arch = "wasm32")]
async fn do_post<R: HostRoute>(
    http: &reqwest::Client,
    url: &str,
    req: &R::Req,
) -> Result<R::Resp, R::Err>
where
    R::Req: Sync,
    R::Resp: Send,
    R::Err: Send,
{
    post_and_parse::<R::Resp>(http, url, req)
        .await
        .map_err(R::Err::from)
}

// ── inner: shared POST + JSON-decode ──────────────────────────────────────────

async fn post_and_parse<Resp: DeserializeOwned + Send>(
    http: &reqwest::Client,
    url: &str,
    req: &(impl Serialize + Sync),
) -> Result<Resp, TransportError> {
    let text = http
        .post(url)
        .json(req)
        .send()
        .await
        .map_err(TransportError::Http)?
        .text()
        .await
        .map_err(TransportError::Http)?;
    serde_json::from_str::<Resp>(&text).map_err(TransportError::Json)
}
