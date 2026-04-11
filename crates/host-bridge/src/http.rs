//! Drop-in replacement for `reqwest::Client` that routes through the host
//! bridge on `wasm32-unknown-unknown`.
//!
//! ## Why
//!
//! Plugins (matrix, stoat, hackernews, …) talk to remote messenger APIs over
//! HTTP. On native, `reqwest::Client` does the right thing. On wasm32 it
//! compiles to `fetch`, which:
//!
//! - leaks the WebView's User-Agent (we can't override it)
//! - sends an `Origin` header and triggers CORS preflight
//! - blocks "forbidden header" names (`Cookie`, `Referer`, `User-Agent`,
//!   `Sec-*`)
//! - applies SameSite cookie rules
//!
//! For a unified messenger app pretending to be a desktop client to multiple
//! servers, that's exactly the wrong stack. [`HttpClient`] re-uses the same
//! API shape as `reqwest::Client` but, when running inside a Poly native
//! shell (Wry / Electron / future iOS / Android), forwards every request
//! over the host bridge as a [`HostCall::HttpRequest`]. The shell makes the
//! actual call with native reqwest — no fetch, no CORS, full header control,
//! whatever User-Agent we want.
//!
//! In a real browser (apps/web with no native shell) the bridge is not
//! reachable; callers can opt into the [`HttpClient::direct`] fallback if
//! they want to keep working with the browser limitations, or fail loud.
//!
//! ## API surface
//!
//! Mirrors a small subset of `reqwest`:
//!
//! - [`HttpClient::get`] / `post` / `put` / `delete` / `patch` / `request`
//! - [`RequestBuilder::header`] / `headers` / `body` / `json` / `bearer_auth`
//!   / `basic_auth` / `query` / `form` / `send`
//! - [`Response::status`] / `headers` / `bytes` / `text` / `json` /
//!   `error_for_status`
//!
//! Method, StatusCode, and HeaderMap are re-exported from `reqwest` so
//! callers don't need to import both crates.

use std::time::Duration;

use bytes::Bytes;
pub use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
pub use reqwest::{Method, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

use crate::{BridgeError, Client as BridgeClient, HostCall, HostOk};

/// Errors returned by [`HttpClient`].
///
/// Intentionally string-based: we wrap a heterogeneous mix of native
/// `reqwest::Error`, `serde_json::Error`, and bridge transport failures, and
/// none of them survive the WASM round-trip in their original form anyway.
#[derive(Debug, Error)]
pub enum HttpError {
    /// The request couldn't be built (bad header value, query encoding error,
    /// JSON serialization failure).
    #[error("invalid request: {0}")]
    Build(String),
    /// Underlying HTTP transport failed (DNS, TLS, connection reset, …) or
    /// the host bridge wasn't reachable.
    #[error("transport error: {0}")]
    Transport(String),
    /// The response body couldn't be decoded as the expected type
    /// (UTF-8 / JSON / …).
    #[error("decode error: {0}")]
    Decode(String),
    /// `error_for_status` was called and the response had a 4xx/5xx status.
    #[error("HTTP status {status}")]
    Status {
        /// The HTTP status code.
        status: StatusCode,
        /// The response body for diagnostics.
        body: String,
    },
}

impl HttpError {
    /// Returns the HTTP status code if this is a `Status` error.
    #[must_use]
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Status { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// HTTP client with the same shape as `reqwest::Client`, but routed through
/// the host bridge on `wasm32-unknown-unknown` so plugins don't inherit the
/// browser fetch sandbox.
#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: HttpInner,
}

#[derive(Debug, Clone)]
enum HttpInner {
    /// Direct reqwest — used on native, and on wasm32 via [`HttpClient::direct`].
    Direct(reqwest::Client),
    /// Bridge-routed — wasm32 default. Each request becomes a
    /// [`HostCall::HttpRequest`] sent to a native shell.
    Bridge(BridgeClient),
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    /// Build the default client for the current target.
    ///
    /// - Native: thin wrapper over `reqwest::Client::new()`.
    /// - wasm32-unknown-unknown: routes through the host bridge at
    ///   [`crate::BRIDGE_URL`]. Use [`HttpClient::direct`] if you need to
    ///   bypass the bridge (apps/web in a real browser).
    #[must_use]
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self {
                inner: HttpInner::Direct(reqwest::Client::new()),
            }
        }
        #[cfg(all(target_arch = "wasm32", feature = "web-direct"))]
        {
            Self {
                inner: HttpInner::Direct(reqwest::Client::new()),
            }
        }
        #[cfg(all(target_arch = "wasm32", not(feature = "web-direct")))]
        {
            Self {
                inner: HttpInner::Bridge(BridgeClient::new()),
            }
        }
    }

    /// Force a direct (non-bridge) reqwest client even on wasm32. On wasm32
    /// this falls back to browser fetch with all the limitations that
    /// implies — only useful for apps/web running in a real browser.
    #[must_use]
    pub fn direct() -> Self {
        Self {
            inner: HttpInner::Direct(reqwest::Client::new()),
        }
    }

    /// Force routing through a specific bridge client. Mostly useful for
    /// tests that want to point at a non-default bridge URL.
    #[must_use]
    pub fn with_bridge(client: BridgeClient) -> Self {
        Self {
            inner: HttpInner::Bridge(client),
        }
    }

    /// Build a `GET` request.
    pub fn get(&self, url: impl Into<String>) -> RequestBuilder {
        self.request(Method::GET, url)
    }

    /// Build a `POST` request.
    pub fn post(&self, url: impl Into<String>) -> RequestBuilder {
        self.request(Method::POST, url)
    }

    /// Build a `PUT` request.
    pub fn put(&self, url: impl Into<String>) -> RequestBuilder {
        self.request(Method::PUT, url)
    }

    /// Build a `DELETE` request.
    pub fn delete(&self, url: impl Into<String>) -> RequestBuilder {
        self.request(Method::DELETE, url)
    }

    /// Build a `PATCH` request.
    pub fn patch(&self, url: impl Into<String>) -> RequestBuilder {
        self.request(Method::PATCH, url)
    }

    /// Build a `HEAD` request.
    pub fn head(&self, url: impl Into<String>) -> RequestBuilder {
        self.request(Method::HEAD, url)
    }

    /// Build a request with an arbitrary method.
    pub fn request(&self, method: Method, url: impl Into<String>) -> RequestBuilder {
        RequestBuilder {
            transport: self.inner.clone(),
            method,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            error: None,
        }
    }
}

/// Builder for [`HttpClient`] with timeout / user-agent / transport overrides.
#[derive(Debug, Default)]
pub struct HttpClientBuilder {
    user_agent: Option<String>,
    timeout: Option<Duration>,
    force_direct: bool,
}

impl HttpClientBuilder {
    /// Start a fresh builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the User-Agent header for all requests.
    ///
    /// **Note:** on the bridge transport this header is added per-request
    /// in the wire payload, so the *native shell* sends it (browser fetch
    /// can't override User-Agent — that's exactly why the bridge exists).
    #[must_use]
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// Request timeout. Honoured on the direct transport; the bridge
    /// transport currently ignores it (the native shell uses its own
    /// reqwest defaults).
    #[must_use]
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(dur);
        self
    }

    /// Force the direct transport even on wasm32 (browser fetch fallback).
    #[must_use]
    pub fn direct(mut self) -> Self {
        self.force_direct = true;
        self
    }

    /// Build the configured client.
    pub fn build(self) -> Result<HttpClient, HttpError> {
        let _ = self.user_agent; // tracked but only consumed by Direct path below
        let use_direct = self.force_direct
            || cfg!(not(target_arch = "wasm32"))
            || cfg!(all(target_arch = "wasm32", feature = "web-direct"));
        if use_direct {
            let mut builder = reqwest::Client::builder();
            if let Some(ua) = self.user_agent {
                builder = builder.user_agent(ua);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(timeout) = self.timeout {
                builder = builder.timeout(timeout);
            }
            let client = builder.build().map_err(|e| HttpError::Build(e.to_string()))?;
            Ok(HttpClient {
                inner: HttpInner::Direct(client),
            })
        } else {
            Ok(HttpClient {
                inner: HttpInner::Bridge(BridgeClient::new()),
            })
        }
    }
}

/// One in-flight HTTP request being built.
///
/// Mirrors `reqwest::RequestBuilder` — fluent setters that return `Self`,
/// terminated by [`RequestBuilder::send`]. State is buffered until `send()`
/// so query/header/body can be set in any order.
pub struct RequestBuilder {
    transport: HttpInner,
    method: Method,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    error: Option<HttpError>,
}

impl RequestBuilder {
    /// Inspect the buffered URL (post-`query`). Mainly for tests that want to
    /// assert URL normalization without actually sending the request.
    #[must_use]
    pub fn url_ref(&self) -> &str {
        &self.url
    }

    /// Look up a single buffered header value by name (case-insensitive).
    /// Returns the first match. Mainly for tests asserting that auth headers
    /// were applied.
    #[must_use]
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Add a single header.
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        if self.error.is_none() {
            self.headers.push((name.into(), value.into()));
        }
        self
    }

    /// Replace the headers entirely with a `HeaderMap`.
    #[must_use]
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        if self.error.is_some() {
            return self;
        }
        self.headers.clear();
        for (name, value) in headers.iter() {
            let Ok(value_str) = value.to_str() else {
                self.error =
                    Some(HttpError::Build(format!("non-ASCII header value for {name}")));
                return self;
            };
            self.headers
                .push((name.as_str().to_string(), value_str.to_string()));
        }
        self
    }

    /// Add an `Authorization: Bearer …` header.
    #[must_use]
    pub fn bearer_auth(self, token: impl std::fmt::Display) -> Self {
        self.header("authorization", format!("Bearer {token}"))
    }

    /// Add an `Authorization: Basic …` header.
    #[must_use]
    pub fn basic_auth<U, P>(self, username: U, password: Option<P>) -> Self
    where
        U: std::fmt::Display,
        P: std::fmt::Display,
    {
        use base64::Engine as _;
        let raw = match password {
            Some(p) => format!("{username}:{p}"),
            None => format!("{username}:"),
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(raw.as_bytes());
        self.header("authorization", format!("Basic {encoded}"))
    }

    /// Set a raw body. Accepts anything that converts into `Vec<u8>` —
    /// `String`, `&str`, `Vec<u8>`, `Bytes`.
    #[must_use]
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        if self.error.is_none() {
            self.body = Some(body.into());
        }
        self
    }

    /// Serialize a value as JSON, set the body, and apply
    /// `content-type: application/json`.
    #[must_use]
    pub fn json<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
        if self.error.is_some() {
            return self;
        }
        let bytes = match serde_json::to_vec(value) {
            Ok(b) => b,
            Err(e) => {
                self.error = Some(HttpError::Build(format!("json serialize: {e}")));
                return self;
            }
        };
        self.header("content-type", "application/json").body(bytes)
    }

    /// Serialize a value as `application/x-www-form-urlencoded` and apply the
    /// matching content-type.
    #[must_use]
    pub fn form<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
        if self.error.is_some() {
            return self;
        }
        let body = match serde_urlencoded::to_string(value) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(HttpError::Build(format!("form serialize: {e}")));
                return self;
            }
        };
        self.header("content-type", "application/x-www-form-urlencoded")
            .body(body.into_bytes())
    }

    /// Append URL query parameters from a serializable value.
    ///
    /// Encodes via `serde_urlencoded` directly because the workspace builds
    /// reqwest with `default-features = false`, which strips reqwest's own
    /// `query` feature.
    #[must_use]
    pub fn query<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
        if self.error.is_some() {
            return self;
        }
        let qs = match serde_urlencoded::to_string(value) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(HttpError::Build(format!("query serialize: {e}")));
                return self;
            }
        };
        if !qs.is_empty() {
            let sep = if self.url.contains('?') { '&' } else { '?' };
            self.url.push(sep);
            self.url.push_str(&qs);
        }
        self
    }

    /// Send the request and decode the response into a [`Response`].
    pub async fn send(self) -> Result<Response, HttpError> {
        if let Some(e) = self.error {
            return Err(e);
        }
        match self.transport {
            HttpInner::Direct(client) => {
                let mut req = client.request(self.method, &self.url);
                for (k, v) in &self.headers {
                    req = req.header(k.as_str(), v.as_str());
                }
                if let Some(body) = self.body {
                    req = req.body(body);
                }
                let resp = req
                    .send()
                    .await
                    .map_err(|e| HttpError::Transport(e.to_string()))?;
                let status = resp.status();
                let headers = resp.headers().clone();
                let body = resp
                    .bytes()
                    .await
                    .map_err(|e| HttpError::Transport(e.to_string()))?;
                Ok(Response {
                    status,
                    headers,
                    body,
                    url: self.url,
                })
            }
            HttpInner::Bridge(client) => {
                let body_b64 = self.body.as_deref().map(b64_encode);
                let call = HostCall::HttpRequest {
                    method: self.method.as_str().to_string(),
                    url: self.url.clone(),
                    headers: self.headers,
                    body_b64,
                };
                let ok = client.call(call).await.map_err(|e| match e {
                    BridgeError::Unreachable { url, source } => HttpError::Transport(format!(
                        "host bridge unreachable at {url}: {source}"
                    )),
                    BridgeError::Host(msg) => HttpError::Transport(msg),
                    other => HttpError::Transport(other.to_string()),
                })?;
                let HostOk::HttpResponse {
                    status,
                    headers,
                    body_b64,
                } = ok
                else {
                    return Err(HttpError::Transport(
                        "bridge returned wrong response variant for http-request".to_string(),
                    ));
                };
                let status = StatusCode::from_u16(status)
                    .map_err(|e| HttpError::Decode(format!("invalid status: {e}")))?;
                let header_map = headers_from_pairs(&headers)?;
                let body = b64_decode(&body_b64).map_err(HttpError::Decode)?;
                Ok(Response {
                    status,
                    headers: header_map,
                    body: Bytes::from(body),
                    url: self.url,
                })
            }
        }
    }
}

fn headers_from_pairs(pairs: &[(String, String)]) -> Result<HeaderMap, HttpError> {
    let mut map = HeaderMap::with_capacity(pairs.len());
    for (k, v) in pairs {
        let name = HeaderName::from_bytes(k.as_bytes())
            .map_err(|e| HttpError::Decode(format!("invalid header name {k}: {e}")))?;
        let value = HeaderValue::from_str(v)
            .map_err(|e| HttpError::Decode(format!("invalid header value for {k}: {e}")))?;
        map.append(name, value);
    }
    Ok(map)
}

fn b64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())
}

/// HTTP response — same shape as `reqwest::Response` for the methods plugins
/// actually use.
#[derive(Debug, Clone)]
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
    url: String,
}

impl Response {
    /// HTTP status code.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Response headers.
    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The URL the request was sent to (post-redirect URL is **not**
    /// tracked — same as `reqwest`'s `Response::url` would be on the
    /// non-redirect path).
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the body length in bytes.
    #[must_use]
    pub fn content_length(&self) -> Option<u64> {
        Some(self.body.len() as u64)
    }

    /// Consume the response and return the raw body bytes.
    pub async fn bytes(self) -> Result<Bytes, HttpError> {
        Ok(self.body)
    }

    /// Consume the response and return the body as UTF-8 text.
    pub async fn text(self) -> Result<String, HttpError> {
        String::from_utf8(self.body.to_vec())
            .map_err(|e| HttpError::Decode(format!("not valid UTF-8: {e}")))
    }

    /// Consume the response and decode the body as JSON.
    pub async fn json<T: DeserializeOwned>(self) -> Result<T, HttpError> {
        serde_json::from_slice(&self.body).map_err(|e| HttpError::Decode(e.to_string()))
    }

    /// Return `Err(HttpError::Status)` if the status is 4xx or 5xx.
    pub fn error_for_status(self) -> Result<Self, HttpError> {
        if self.status.is_client_error() || self.status.is_server_error() {
            let body = String::from_utf8_lossy(&self.body).into_owned();
            Err(HttpError::Status {
                status: self.status,
                body,
            })
        } else {
            Ok(self)
        }
    }
}
