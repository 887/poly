//! Cross-target async sleep.
//!
//! `poly-teams` is **not** target-gated out of the browser bundle:
//! `crates/core/Cargo.toml` pulls it in with default features (`native`), so
//! every `native`-gated module here is also compiled for
//! `wasm32-unknown-unknown` when `apps/web` / `apps/desktop-electron` build
//! with `dev-plugins`.
//!
//! On that target `tokio::time::sleep` and `std::time::Instant::now()` are
//! unimplemented and panic at runtime (`time not implemented on this
//! platform`). WASM has no unwinding, so the panic aborts the whole module —
//! CLAUDE.md hang class #4. Route every delay through [`sleep`] instead of
//! calling `tokio::time` directly.

use std::time::Duration;

/// Await `duration`, using the runtime timer that actually exists on this
/// target.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// Browser variant — `setTimeout` as a Rust future.
///
/// `gloo_timers` takes a `u32` millisecond budget; saturate on overflow rather
/// than truncating (a `Duration` past `u32::MAX` ms is ~49 days, far beyond any
/// backoff we schedule).
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub(crate) async fn sleep(duration: Duration) {
    let ms = u32::try_from(duration.as_millis()).unwrap_or(u32::MAX);
    gloo_timers::future::TimeoutFuture::new(ms).await;
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Duration;

    #[tokio::test]
    async fn sleep_completes() {
        super::sleep(Duration::from_millis(1)).await;
    }

    #[tokio::test]
    async fn zero_sleep_completes() {
        super::sleep(Duration::ZERO).await;
    }
}
