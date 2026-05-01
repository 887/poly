//! Header inspection ring buffer — shared across all test-server backends.
//!
//! ## Design
//!
//! Every inbound HTTP request's method, path, and headers are appended to a
//! [`HeaderInspectBuffer`] capped at [`HEADER_INSPECT_CAP`] entries (FIFO
//! eviction). The buffer is wired into each backend's router via
//! [`header_inspect_layer`], and exposed read-only via
//! [`handle_inspect_last_headers`] at `GET /test/inspect/last-headers`.
//!
//! ## Reset policy
//!
//! When a backend's `/reset` handler runs it should call
//! [`HeaderInspectBuffer::clear`] so the buffer reflects only requests made
//! after the reset. This keeps assertion windows clean between test cases.
//!
//! ## Cap
//!
//! N=100 entries, FIFO eviction. Under long e2e runs the buffer never grows
//! past this limit even with many concurrent requests.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// Maximum number of entries retained by the ring buffer.
///
/// Exposed so tests can reference the constant without a magic number:
/// `assert!(entries.len() <= HEADER_INSPECT_CAP);`
pub const HEADER_INSPECT_CAP: usize = 100;

/// A single captured inbound request entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeaderEntry {
    /// HTTP method (e.g. `"GET"`, `"POST"`).
    pub method: String,
    /// Request path including query string (e.g. `"/api/v10/guilds/1/channels"`).
    pub path: String,
    /// All request headers, lower-cased names.
    pub headers: HashMap<String, String>,
    /// Timestamp when the entry was captured (UTC).
    pub captured_at: DateTime<Utc>,
}

/// Ring buffer of inbound request headers, capped at [`HEADER_INSPECT_CAP`].
///
/// Thread-safe via an interior `Mutex`; cheaply `Clone`-able because the inner
/// buffer is `Arc`-wrapped.
#[derive(Clone, Debug, Default)]
pub struct HeaderInspectBuffer {
    inner: Arc<Mutex<VecDeque<HeaderEntry>>>,
}

impl HeaderInspectBuffer {
    /// Create an empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Append a new entry.  Evicts the oldest entry once the cap is reached.
    pub fn record(&self, method: impl Into<String>, path: impl Into<String>, headers: HashMap<String, String>) {
        let entry = HeaderEntry {
            method: method.into(),
            path: path.into(),
            headers,
            captured_at: Utc::now(),
        };
        let mut buf = self.inner.lock().expect("inspect buffer lock poisoned");
        if buf.len() == HEADER_INSPECT_CAP {
            buf.pop_front();
        }
        buf.push_back(entry);
    }

    /// Return all captured entries, most-recent first.
    #[must_use]
    pub fn recent(&self) -> Vec<HeaderEntry> {
        let buf = self.inner.lock().expect("inspect buffer lock poisoned");
        buf.iter().rev().cloned().collect()
    }

    /// Discard all captured entries.
    ///
    /// Call this from each backend's `/reset` handler so the buffer reflects
    /// only post-reset requests.
    pub fn clear(&self) {
        let mut buf = self.inner.lock().expect("inspect buffer lock poisoned");
        buf.clear();
    }
}

// ---------------------------------------------------------------------------
// Axum middleware
// ---------------------------------------------------------------------------

/// Axum middleware that records every inbound request into a
/// [`HeaderInspectBuffer`] before passing it to the next handler.
///
/// Wire this into a router with:
/// ```ignore
/// router.layer(axum::middleware::from_fn_with_state(
///     Arc::clone(&state.inspect),
///     header_inspect_middleware,
/// ))
/// ```
pub async fn header_inspect_middleware(
    State(buf): State<Arc<HeaderInspectBuffer>>,
    req: Request<Body>,
    next: Next,
) -> impl IntoResponse {
    let method = req.method().to_string();
    let path = req.uri().path_and_query().map_or_else(
        || req.uri().path().to_string(),
        |pq| pq.as_str().to_string(),
    );

    let headers: HashMap<String, String> = req
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_lowercase(),
                v.to_str().unwrap_or("<non-utf8>").to_string(),
            )
        })
        .collect();

    buf.record(method, path, headers);

    next.run(req).await
}

// ---------------------------------------------------------------------------
// Route handler
// ---------------------------------------------------------------------------

/// `GET /test/inspect/last-headers` — return the ring buffer as JSON.
///
/// Response is an array of [`HeaderEntry`] objects, most-recent first, capped
/// at [`HEADER_INSPECT_CAP`] total entries.
pub async fn handle_inspect_last_headers(
    State(buf): State<Arc<HeaderInspectBuffer>>,
) -> Json<Vec<HeaderEntry>> {
    Json(buf.recent())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn make_headers(ua: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("user-agent".to_string(), ua.to_string());
        m
    }

    #[test]
    fn test_ring_buffer_cap_stays_at_100() {
        let buf = HeaderInspectBuffer::new();
        // Send 200 entries — twice the cap.
        for i in 0..200u32 {
            buf.record("GET", format!("/test/{i}"), make_headers("bot/1.0"));
        }
        let entries = buf.recent();
        assert!(
            entries.len() <= HEADER_INSPECT_CAP,
            "buffer exceeded cap: {} > {}",
            entries.len(),
            HEADER_INSPECT_CAP
        );
        assert_eq!(entries.len(), HEADER_INSPECT_CAP);
        // Most-recent first: the 200th request should be first.
        assert_eq!(entries[0].path, "/test/199");
        // Oldest retained should be the 100th.
        assert_eq!(entries[HEADER_INSPECT_CAP - 1].path, "/test/100");
    }

    #[test]
    fn test_recent_returns_most_recent_first() {
        let buf = HeaderInspectBuffer::new();
        buf.record("GET", "/first", make_headers("a"));
        buf.record("POST", "/second", make_headers("b"));
        buf.record("DELETE", "/third", make_headers("c"));

        let entries = buf.recent();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].path, "/third");
        assert_eq!(entries[1].path, "/second");
        assert_eq!(entries[2].path, "/first");
    }

    #[test]
    fn test_clear_empties_buffer() {
        let buf = HeaderInspectBuffer::new();
        buf.record("GET", "/x", make_headers("x"));
        assert_eq!(buf.recent().len(), 1);
        buf.clear();
        assert_eq!(buf.recent().len(), 0);
    }

    #[test]
    fn test_headers_captured_liberally() {
        let mut h = HashMap::new();
        h.insert("user-agent".to_string(), "MyClient/1.0".to_string());
        h.insert("x-super-properties".to_string(), "base64data".to_string());
        h.insert("authorization".to_string(), "Bot test-token".to_string());
        buf_record_and_check(h);
    }

    fn buf_record_and_check(headers: HashMap<String, String>) {
        let buf = HeaderInspectBuffer::new();
        buf.record("GET", "/api/test", headers.clone());
        let entries = buf.recent();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.method, "GET");
        assert_eq!(entry.path, "/api/test");
        for (k, v) in &headers {
            assert_eq!(entry.headers.get(k).unwrap(), v, "header {k} missing or wrong");
        }
    }
}
