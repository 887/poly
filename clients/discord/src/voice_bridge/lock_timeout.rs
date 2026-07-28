//! Bounded `RwLock` reads for the WASM voice-bridge — hang class #4.
//!
//! `voice_bridge` compiles for `wasm32-unknown-unknown`, where the whole async
//! runtime is one thread. A `tokio::sync::RwLock::read().await` that loses to a
//! perpetual writer there does not merely block a task — it starves the browser
//! main thread and wedges the tab (CLAUDE.md hang class #4).
//!
//! The canonical countermeasure elsewhere in the codebase is
//! `BackendHandleExt::read_with_timeout`, but that trait is defined over backend
//! handles. These are plain collection locks, so this module provides the same
//! guarantee for a bare `RwLock`.
//!
//! **`tokio::time::timeout` must not be used on wasm32** — it calls
//! `Instant::now()`, which is unimplemented on that target and panics. The
//! cfg-split below mirrors `voice_protocol::WsHandle::recv_text_with_timeout`:
//! `gloo_timers::future::TimeoutFuture` raced via `futures::future::select` on
//! wasm, `tokio::time::timeout` on native.

use std::time::Duration;

use tokio::sync::{RwLock, RwLockReadGuard};

/// Default bound for voice-path lock reads. Matches the 5s used by
/// `BackendHandleExt::read_with_timeout`.
pub const LOCK_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Acquire a read guard, or `None` if `dur` elapses first.
///
/// Callers MUST have a real degradation branch for `None` — silently retrying
/// or looping reintroduces the starvation this exists to prevent.
pub async fn read_with_timeout<T>(
    lock: &RwLock<T>,
    dur: Duration,
) -> Option<RwLockReadGuard<'_, T>> {
    #[cfg(target_arch = "wasm32")]
    {
        use futures::future::{select, Either};
        let timeout = gloo_timers::future::TimeoutFuture::new(
            u32::try_from(dur.as_millis()).unwrap_or(u32::MAX),
        );
        let read = lock.read();
        futures::pin_mut!(timeout);
        futures::pin_mut!(read);
        match select(timeout, read).await {
            Either::Left(_) => None,
            Either::Right((guard, _)) => Some(guard),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::timeout(dur, lock.read()).await.ok()
    }
}

#[cfg(test)]
// No `#[expect(unwrap_used/expect_used/panic)]` header here: these tests use
// only `assert!` / `assert_eq!`, so the expectation was unfulfilled (which is
// itself a clippy error under `-D warnings`).
mod tests {
    use super::*;

    #[tokio::test]
    async fn acquires_an_uncontended_lock() {
        let lock = RwLock::new(7_u32);
        let guard = read_with_timeout(&lock, LOCK_READ_TIMEOUT).await;
        assert_eq!(guard.map(|g| *g), Some(7));
    }

    #[tokio::test]
    async fn concurrent_readers_do_not_block_each_other() {
        let lock = RwLock::new(1_u32);
        let a = read_with_timeout(&lock, LOCK_READ_TIMEOUT).await;
        let b = read_with_timeout(&lock, LOCK_READ_TIMEOUT).await;
        assert!(a.is_some() && b.is_some(), "read locks are shared");
    }

    #[tokio::test]
    async fn returns_none_when_a_writer_holds_the_lock() {
        let lock = RwLock::new(0_u32);
        let _writer = lock.write().await;
        // This is the starvation case: without the bound, this await never
        // returns and on wasm32 it takes the browser main thread with it.
        let guard = read_with_timeout(&lock, Duration::from_millis(50)).await;
        assert!(guard.is_none(), "must time out rather than block forever");
    }
}
