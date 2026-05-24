//! Stoat WASM video transport surface (Phase B.5 of `plan-stoat-video-wasm.md`).
//!
//! Exposes `start_video_capture` and `stop_video_capture` as inherent methods on
//! [`StoatClient`] so the UI layer can drive camera capture without reaching into
//! [`crate::video_wasm_capture`] directly.
//!
//! # Design rationale
//!
//! A new cross-crate `VideoTransportBackend` trait is out of scope for B.5 — that
//! expansion is its own plan entry. Instead these methods live on `StoatClient`
//! directly (same pattern as `set_noise_cancel`, `connect_voice`, etc.), which is
//! sufficient for the UI: it holds a concrete `StoatClient` reference and can call
//! `start_video_capture(channel_id)` / `stop_video_capture()` without a trait
//! dispatch.
//!
//! Both methods are `wasm32`-only (gated by the `#[cfg(target_arch = "wasm32")]`
//! on the module declaration in `lib.rs`).

use poly_client::ClientError;

use super::{StoatClient, video_wasm_capture};

impl StoatClient {
    /// Start WASM camera capture and route H.264 frames through the live Vortex WS.
    ///
    /// Acquires the camera via `getUserMedia({video:…})`, spawns the FU-A fragment
    /// loop, and stores the resulting [`video_wasm_capture::StoatVideoCaptureHandle`]
    /// in `self.video_wasm_conn` so the camera stays open until
    /// [`Self::stop_video_capture`] is called.
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotSupported`] — no active voice connection (voice WS must
    ///   be opened first via [`super::voice_transport`] /
    ///   `join_voice_channel_transport`).
    /// - [`ClientError::Internal`] — `getUserMedia` rejected, `VideoEncoder` not
    ///   available, or lock poisoned.
    ///
    /// Calling while a capture is already active replaces the old handle (the old
    /// camera track is stopped via `StoatVideoCaptureHandle::stop` before the new
    /// one starts).
    pub async fn start_video_capture(&self, _channel_id: &str) -> Result<(), ClientError> {
        // Borrow the WS sender from the live voice connection.
        let (ws_tx, shutdown) = {
            let guard = self.voice_wasm_conn.lock().map_err(|e| {
                ClientError::Internal(format!("video_transport: voice_wasm_conn lock: {e}"))
            })?;
            let conn = guard.as_ref().ok_or_else(|| {
                ClientError::NotSupported(
                    "start_video_capture: no active voice connection — join a voice channel first"
                        .into(),
                )
            })?;
            (conn.ws_sender(), conn.shutdown_flag())
        };

        // Stop any existing capture session before starting a new one.
        {
            let mut cap_guard = self.video_wasm_conn.lock().map_err(|e| {
                ClientError::Internal(format!("video_transport: video_wasm_conn lock: {e}"))
            })?;
            if let Some(old) = cap_guard.take() {
                old.stop();
            }
        }

        let handle = video_wasm_capture::start_video_capture(ws_tx, shutdown)
            .await
            .map_err(|e| ClientError::Internal(format!("start_video_capture: {e:?}")))?;

        if let Ok(mut cap_guard) = self.video_wasm_conn.lock() {
            *cap_guard = Some(handle);
        }

        Ok(())
    }

    /// Stop the running WASM camera capture session, if any.
    ///
    /// Signals the capture task to stop (the camera is released and no further
    /// video frames are sent). No-op if no capture is active.
    pub fn stop_video_capture(&self) {
        if let Ok(mut guard) = self.video_wasm_conn.lock() {
            if let Some(handle) = guard.take() {
                handle.stop();
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
// lint-allow-unused: test module uses unwrap/expect/panic per project policy
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::StoatClient;

    /// stop_video_capture is a no-op when no capture is active.
    #[test]
    fn stop_video_capture_no_op_when_inactive() {
        let client = StoatClient::new();
        // Must not panic or deadlock.
        client.stop_video_capture();
        // State remains None.
        let guard = client.video_wasm_conn.lock().unwrap();
        assert!(guard.is_none());
    }

    /// start_video_capture returns NotSupported when no voice connection is active.
    #[tokio::test]
    async fn start_video_capture_requires_voice_connection() {
        let client = StoatClient::new();
        let result = client.start_video_capture("CH001").await;
        assert!(
            matches!(result, Err(ClientError::NotSupported(_))),
            "expected NotSupported, got {result:?}"
        );
    }
}
