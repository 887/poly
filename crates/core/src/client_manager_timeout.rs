//! `BackendHandle::read_with_timeout` — Hang #4 prevention (Phase 1).
//!
//! CLAUDE.md § "Common WASM-hang causes" item #4 flags
//! `tokio::sync::RwLock::read().await` on a backend with a perpetual writer
//! as a way to wedge the single-threaded WASM scheduler. The naive guard —
//! `tokio::time::timeout(dur, backend.read()).await` — **panics on WASM**
//! because `Instant::now()` is unimplemented on `wasm32-unknown-unknown`.
//! Four in-tree comments still document prior removed attempts:
//!
//! - `crates/core/src/ui/account/common/channel_list.rs:193-195`
//! - `crates/core/src/ui/account/common/channel_list.rs:360-364`
//! - `crates/core/src/ui/routes.rs:1067-1069`
//! - `crates/core/src/ui/account/common/draft_banner.rs:168-170`
//!
//! This module supplies a cfg-gated wrapper that works on both targets:
//!
//! - native → `tokio::time::timeout`
//! - wasm32 → race the `read()` future against
//!   `gloo_timers::future::TimeoutFuture`.
//!
//! Call sites switch from `backend.read().await` to
//! `backend.read_with_timeout(Duration::from_secs(5)).await?`. Phase 2 of
//! `docs/plans/plan-backend-read-timeout.md` handles the migration.

use std::fmt;
use std::panic::Location;
use std::time::Duration;

use poly_client::ClientBackend;
use tokio::sync::RwLockReadGuard;

use crate::client_manager::BackendHandle;

/// Error returned when `read_with_timeout` exceeds its budget.
///
/// Unit-ish (holds duration + location for the tracing warn). Construction
/// is deliberately internal so the value always reflects a real timeout
/// observed by the helper — not a caller-fabricated error.
#[derive(Debug, Clone, Copy)]
pub struct BackendReadTimeout {
    duration: Duration,
    location: &'static Location<'static>,
}

impl BackendReadTimeout {
    /// Original budget the caller passed in.
    #[must_use] 
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// Source location of the `read_with_timeout` call site.
    #[must_use] 
    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }
}

impl fmt::Display for BackendReadTimeout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "backend read timed out after {}ms at {}",
            self.duration.as_millis(),
            self.location,
        )
    }
}

impl std::error::Error for BackendReadTimeout {}

/// Extension trait adding a WASM-safe timeout to `BackendHandle::read()`.
///
/// Preferred over raw `backend.read().await` at every UI call site — see
/// `docs/plans/plan-backend-read-timeout.md` §2.
pub trait BackendHandleExt {
    /// Acquire a read guard, bounded by `duration`.
    ///
    /// On timeout, emits a `tracing::warn!` tagged with the call site
    /// location (via `#[track_caller]`) and returns
    /// [`BackendReadTimeout`]. Callers should treat timeout as recoverable
    /// — bail the effect, reset any `loading` flags, and let the user
    /// retry.
    #[track_caller]
    fn read_with_timeout(
        &self,
        duration: Duration,
    ) -> impl std::future::Future<
        Output = Result<RwLockReadGuard<'_, Box<dyn ClientBackend>>, BackendReadTimeout>,
    >;
}

impl BackendHandleExt for BackendHandle {
    #[track_caller]
    fn read_with_timeout(
        &self,
        duration: Duration,
    ) -> impl std::future::Future<
        Output = Result<RwLockReadGuard<'_, Box<dyn ClientBackend>>, BackendReadTimeout>,
    > {
        let location = Location::caller();
        read_with_timeout_impl(self, duration, location)
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn read_with_timeout_impl<'a>(
    handle: &'a BackendHandle,
    duration: Duration,
    location: &'static Location<'static>,
) -> Result<RwLockReadGuard<'a, Box<dyn ClientBackend>>, BackendReadTimeout> {
    match tokio::time::timeout(duration, handle.read()).await {
        Ok(guard) => Ok(guard),
        Err(_) => {
            let err = BackendReadTimeout { duration, location };
            tracing::warn!(
                target: "poly_core::backend_timeout",
                duration_ms = duration.as_millis() as u64,
                location = %location,
                "backend read timed out — see CLAUDE.md hang class #4"
            );
            Err(err)
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn read_with_timeout_impl<'a>(
    handle: &'a BackendHandle,
    duration: Duration,
    location: &'static Location<'static>,
) -> Result<RwLockReadGuard<'a, Box<dyn ClientBackend>>, BackendReadTimeout> {
    use futures::future::{select, Either};

    // `gloo_timers` takes a u32 milliseconds budget. Saturate on overflow —
    // a Duration too large for u32 ms is effectively "no timeout".
    let ms: u32 = u32::try_from(duration.as_millis()).unwrap_or(u32::MAX);
    let timer = gloo_timers::future::TimeoutFuture::new(ms);
    let read_fut = handle.read();

    // Both futures are on the stack of this async fn; `select` pins them
    // internally.
    match select(Box::pin(read_fut), Box::pin(timer)).await {
        Either::Left((guard, _timer)) => Ok(guard),
        Either::Right(((), _read_fut)) => {
            let err = BackendReadTimeout { duration, location };
            tracing::warn!(
                target: "poly_core::backend_timeout",
                duration_ms = duration.as_millis() as u64,
                location = %location,
                "backend read timed out — see CLAUDE.md hang class #4"
            );
            Err(err)
        }
    }
}

// WASM unit tests are skipped for now — the WASM test harness is fragile.
// The WASM path will be smoke-tested as part of the Phase-2 migration's
// manual QA (permalink jump, history scroll, DM open, channel-list expand).
#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use poly_client::ClientBackend;
    use tokio::sync::RwLock;

    use super::{BackendHandleExt, BackendReadTimeout};
    use crate::client_manager::BackendHandle;

    /// Minimal `ClientBackend` stand-in — we never actually call any trait
    /// method on it; the helper only exercises the `RwLock` surface.
    ///
    /// `ClientBackend` has a large surface area, so rather than implement
    /// it manually we build a demo backend by activating the `poly-demo`
    /// plugin — the `poly-core` crate's `demo` feature is on by default.
    fn make_handle() -> BackendHandle {
        use poly_demo::DemoClient;
        let demo: Box<dyn ClientBackend> = Box::new(DemoClient::new());
        Arc::new(RwLock::new(demo))
    }

    #[tokio::test]
    async fn read_completes_within_timeout() {
        let handle = make_handle();
        let guard = handle
            .read_with_timeout(Duration::from_secs(5))
            .await
            .expect("uncontended read should succeed");
        // Guard is live — just drop it.
        drop(guard);
    }

    #[tokio::test]
    async fn read_times_out_when_writer_held() {
        let handle = make_handle();
        // Take the write lock and hold it — readers can never acquire.
        let write_guard = handle.write().await;

        // `Ok` branch carries `dyn ClientBackend` which isn't Debug, so
        // we can't use `.expect_err` — match instead.
        let err: BackendReadTimeout = match handle
            .read_with_timeout(Duration::from_millis(50))
            .await
        {
            Ok(_) => panic!("read should time out behind a perpetual writer"),
            Err(e) => e,
        };

        assert_eq!(err.duration(), Duration::from_millis(50));
        // Display impl should mention the duration in ms.
        let msg = format!("{}", err);
        assert!(msg.contains("50ms"), "unexpected Display: {msg}");

        drop(write_guard);
    }

    #[tokio::test]
    async fn zero_duration_times_out_immediately() {
        let handle = make_handle();
        // Hold the writer so the read can't win the race even at 0ms.
        let write_guard = handle.write().await;

        let err = match handle
            .read_with_timeout(Duration::from_millis(0))
            .await
        {
            Ok(_) => panic!("0ms budget should always time out when contended"),
            Err(e) => e,
        };
        assert_eq!(err.duration(), Duration::from_millis(0));

        drop(write_guard);
    }
}
