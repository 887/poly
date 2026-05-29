//! `impl VoiceTransportBackend for StoatClient` — Vortex WS join/leave, mute, DM transient calls.
//!
//! Split out from `lib.rs` in SOLID-audit-stoat D.2 (C.1 / G.4 / H.2-H.5 / B.6).
//!
//! NOTE: The voice transport itself (`voice.rs` / `voice_wasm.rs`) is OUT OF SCOPE
//! for this refactor — only the `VoiceTransportBackend` trait impl moves here.
//! The transport modules are referenced via `crate::voice` / `crate::voice_wasm`.

use async_trait::async_trait;
#[cfg(feature = "voice")]
use crate::voice;
#[cfg(target_arch = "wasm32")]
use crate::voice_wasm;
use poly_client::{ClientError, ClientResult, VoiceParticipant};
#[cfg(target_arch = "wasm32")]
use poly_client::ClientEvent;
use poly_host_bridge::http::Method;

use super::StoatClient;

// lint-allow-unused: three cfg-gated platform arms (wasm32, voice feature, fallback) cannot be
// split into smaller functions without losing the cfg context and platform-specific imports.
#[allow(clippy::too_many_lines)]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::VoiceTransportBackend for StoatClient {
    async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        // F.7 — return participants from the voice cache populated by Vortex WS events.
        // Falls back to empty vec when voice feature is not enabled or no active session.
        #[cfg(feature = "voice")]
        {
            let guard = voice::get_voice_participants_cached(&self.voice_guard, _channel_id).await;
            if !guard.is_empty() {
                return Ok(guard);
            }
            // Also check the RwLock cache (populated by event_stream).
            if let Ok(cache) = self.voice_participants.try_read()
                && let Some(participants) = cache.get(_channel_id)
            {
                return Ok(participants.clone());
            }
        }
        Ok(vec![])
    }

    /// G.1 / B.6 — Signal the Stoat backend that the local user is joining a voice channel.
    ///
    /// On **native** (feature = "voice"): calls `POST /channels/{channel_id}/join_call`
    /// for the REST signaling step only; the full Vortex WS transport is started
    /// separately via `StoatClient::connect_voice`.
    ///
    /// On **wasm32**: calls `voice_wasm::connect_voice_wasm`, which performs the
    /// join_call HTTP POST, opens the Vortex WebSocket, and spawns Opus
    /// encode/decode/event loops. The resulting `StoatVoiceConnection` is stored in
    /// `self.voice_wasm_conn` so it isn't dropped (which would tear down all tasks).
    #[cfg_attr(not(any(feature = "voice", target_arch = "wasm32")), allow(unused_variables))]
    async fn join_voice_channel_transport(
        &self,
        _server_id: &str,
        channel_id: &str,
    ) -> ClientResult<()> {
        // ── WASM arm (B.6) ───────────────────────────────────────────────────────
        #[cfg(target_arch = "wasm32")]
        {
            let base_url = self.http.base_url().to_string();
            let auth_token = self
                .session_token()
                .ok_or_else(|| ClientError::AuthFailed("not authenticated".into()))?;

            // An internal event channel: voice events flow into the main event_stream
            // sink elsewhere. Using an unbounded channel is fine — WASM is single-threaded
            // and the receive half is consumed by the decode loop inside connect_voice_wasm.
            let (event_tx, _event_rx) = futures::channel::mpsc::unbounded::<ClientEvent>();

            // B.8 — pass the shared noise-cancel flag to the encode loop.
            // The Arc<AtomicBool> is stored in self.voice_noise_cancel and can be
            // updated at runtime via set_noise_cancel() without reconnecting.
            let noise_cancel = std::sync::Arc::clone(&self.voice_noise_cancel);

            let conn = voice_wasm::connect_voice_wasm(
                channel_id.to_string(),
                base_url,
                auth_token,
                None, // transmit_mode: default (push-to-talk off)
                noise_cancel,
                event_tx,
            )
            .await
            .map_err(|e| ClientError::Internal(format!("Stoat WASM voice: {e:?}")))?;

            // Store the live connection — dropping it would kill all background tasks.
            if let Ok(mut guard) = self.voice_wasm_conn.lock() {
                *guard = Some(conn);
            }

            return Ok(());
        }

        // ── Native arm (feature = "voice") ───────────────────────────────────────
        #[cfg(feature = "voice")]
        {
            // POST /channels/{channel_id}/join_call — tell the Vortex server we're joining.
            let response = self
                .http
                .authenticated_request(Method::POST, &format!("/channels/{channel_id}/join_call"))?
                .json(&serde_json::json!({}))
                .send()
                .await
                .map_err(|e| ClientError::Network(e.to_string()))?;

            if !response.status().is_success() {
                return Err(ClientError::Network(format!(
                    "join_call failed: HTTP {}",
                    response.status()
                )));
            }

            let resp: serde_json::Value = response
                .json()
                .await
                .map_err(|e| ClientError::Network(e.to_string()))?;

            let token = resp.get("token")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ClientError::Internal("join_call: missing token".into()))?
                .to_string();
            let ws_url = resp.get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ClientError::Internal("join_call: missing url".into()))?
                .to_string();

            tracing::info!(channel_id, token = %token, ws_url = %ws_url, "Stoat join_call OK");
            return Ok(());
        }

        // ── Fallback: native build without voice feature ──────────────────────────
        #[cfg(not(any(feature = "voice", target_arch = "wasm32")))]
        Ok(())
    }

    /// H.4 / H.2 — Stoat DM call via synthetic transient voice channel.
    ///
    /// Creates a transient VoiceChannel via `POST /channels/create`, invites the
    /// DM target, stores the mapping in `transient_dm_channels` for H.5 cleanup,
    /// then calls `join_voice_channel_transport` to connect.
    ///
    /// On `"cancel:<dm_id>"` prefix: disconnect active call AND delete the
    /// transient channel (H.5 cleanup).
    async fn start_dm_call_transport(&self, dm_channel_id: &str) -> ClientResult<()> {
        // H.5 / cancel path — disconnect and clean up the transient channel.
        if let Some(real_dm_id) = dm_channel_id.strip_prefix("cancel:") {
            tracing::info!("Stoat DM call cancel for dm_id={real_dm_id}");

            // H.5 — look up and delete the transient channel (best-effort).
            #[cfg(feature = "native")]
            {
                let transient_id = self
                    .transient_dm_channels
                    .lock()
                    .ok()
                    .and_then(|mut map| map.remove(real_dm_id));

                if let Some(ch_id) = transient_id {
                    tracing::info!(
                        dm_id = real_dm_id,
                        transient_channel_id = %ch_id,
                        "H.5: deleting transient voice channel"
                    );
                    let delete_result = self
                        .http
                        .authenticated_request(Method::DELETE, &format!("/channels/{ch_id}"))
                        .map(|req| async move { req.send().await });

                    match delete_result {
                        Ok(fut) => match fut.await {
                            Ok(resp) if resp.status().as_u16() == 204 || resp.status().is_success() => {
                                tracing::info!(transient_channel_id = %ch_id, "H.5: transient channel deleted");
                            }
                            Ok(resp) => {
                                tracing::warn!(
                                    transient_channel_id = %ch_id,
                                    status = resp.status().as_u16(),
                                    "H.5: transient channel DELETE returned non-success (ignored)"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    transient_channel_id = %ch_id,
                                    error = %e,
                                    "H.5: transient channel DELETE failed (ignored)"
                                );
                            }
                        },
                        Err(e) => {
                            tracing::warn!(error = %e, "H.5: could not build DELETE request (ignored)");
                        }
                    }
                }
            }

            #[cfg(feature = "voice")]
            voice::disconnect_voice(std::sync::Arc::clone(&self.voice_guard)).await;
            return Ok(());
        }

        // H.2 — create a transient voice channel for the DM call (native only).
        // WASM fallback: delegate directly with the dm_channel_id (no channel creation).
        #[cfg(feature = "native")]
        {
            // Step 1: look up the DM channel to find the other recipient.
            let dm_channel_resp = self
                .http
                .authenticated_request(Method::GET, &format!("/channels/{dm_channel_id}"))?
                .send()
                .await
                .map_err(|e| ClientError::Network(e.to_string()))?;

            if !dm_channel_resp.status().is_success() {
                return Err(ClientError::Network(format!(
                    "GET /channels/{dm_channel_id} failed: HTTP {}",
                    dm_channel_resp.status()
                )));
            }

            let dm_json: serde_json::Value = dm_channel_resp
                .json()
                .await
                .map_err(|e| ClientError::Network(e.to_string()))?;

            // Extract DM recipients; the "other" user is whoever is not the local user.
            let local_user_id = self
                .http
                .session()
                .and_then(|s| s.user_id)
                .unwrap_or_default();

            let recipients: Vec<String> = dm_json
                .get("recipients")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .filter(|id| *id != local_user_id.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();

            tracing::info!(
                dm_channel_id,
                ?recipients,
                "H.2: creating transient voice channel for DM call"
            );

            // Step 2: POST /channels/create to make a transient VoiceChannel.
            let create_body = serde_json::json!({
                "channel_type": "VoiceChannel",
                "name": format!("dm-call-{dm_channel_id}"),
                "transient": true,
                "recipients": recipients,
            });

            let create_resp = self
                .http
                .authenticated_request(Method::POST, "/channels/create")?
                .json(&create_body)
                .send()
                .await
                .map_err(|e| ClientError::Network(e.to_string()))?;

            if !create_resp.status().is_success() {
                return Err(ClientError::Network(format!(
                    "POST /channels/create failed: HTTP {}",
                    create_resp.status()
                )));
            }

            let create_json: serde_json::Value = create_resp
                .json()
                .await
                .map_err(|e| ClientError::Network(e.to_string()))?;

            let transient_channel_id = create_json
                .get("_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ClientError::Internal("channels/create: missing _id".into()))?
                .to_string();

            tracing::info!(
                dm_channel_id,
                transient_channel_id = %transient_channel_id,
                "H.2: transient voice channel created"
            );

            // Step 3: store the mapping for H.5 cleanup.
            if let Ok(mut map) = self.transient_dm_channels.lock() {
                map.insert(dm_channel_id.to_string(), transient_channel_id.clone());
            }

            // Step 4: join the transient channel via the existing voice transport.
            return <Self as poly_client::VoiceTransportBackend>::join_voice_channel_transport(
                self,
                "",
                &transient_channel_id,
            )
            .await;
        }

        // Fallback: if neither branch returned above (should be unreachable when
        // feature = "native" is active since the block above always returns), propagate
        // a sensible error so the compiler sees a definite return on all paths.
        #[allow(unreachable_code)]
        Err(ClientError::Internal(
            "start_dm_call_transport: unhandled code path".into(),
        ))
    }

    /// G.4 — Mute / deafen via Stoat's `PATCH /channels/{id}/voice_state`.
    ///
    /// Sends `{ "muted": bool, "deafened": bool }` to the Stoat server.
    /// Returns silently if not authenticated. Maps non-204 responses to
    /// `ClientError::Network`. Works on both native and wasm32 (the
    /// `StoatHttpClient::authenticated_request` helper is cfg-agnostic).
    #[cfg_attr(not(any(feature = "voice", target_arch = "wasm32")), allow(unused_variables))]
    async fn set_voice_mute(
        &self,
        _server_id: &str,
        channel_id: &str,
        self_mute: bool,
        self_deaf: bool,
    ) -> ClientResult<()> {
        // ── WASM arm ─────────────────────────────────────────────────────────────
        #[cfg(target_arch = "wasm32")]
        {
            let base_url = self.http.base_url().to_string();
            let auth_token = match self.session_token() {
                Some(t) => t,
                None => {
                    tracing::warn!("set_voice_mute: not authenticated, skipping");
                    return Ok(());
                }
            };
            let url = format!(
                "{}/channels/{}/voice_state",
                base_url.trim_end_matches('/'),
                channel_id
            );
            let body = serde_json::json!({ "muted": self_mute, "deafened": self_deaf });

            let resp = gloo_net::http::Request::patch(&url)
                .header("Authorization", &format!("Bearer {auth_token}"))
                .json(&body)
                .map_err(|e| ClientError::Network(format!("set_voice_mute build: {e:?}")))?
                .send()
                .await
                .map_err(|e| ClientError::Network(format!("set_voice_mute send: {e:?}")))?;

            if resp.status() != 204 && !resp.ok() {
                return Err(ClientError::Network(format!(
                    "PATCH voice_state failed: HTTP {}",
                    resp.status()
                )));
            }
            return Ok(());
        }

        // ── Native arm ───────────────────────────────────────────────────────────
        #[cfg(not(target_arch = "wasm32"))]
        {
            let response = self
                .http
                .authenticated_request(Method::PATCH, &format!("/channels/{channel_id}/voice_state"))?
                .json(&serde_json::json!({ "muted": self_mute, "deafened": self_deaf }))
                .send()
                .await
                .map_err(|e| ClientError::Network(e.to_string()))?;

            if response.status().as_u16() != 204 && !response.status().is_success() {
                return Err(ClientError::Network(format!(
                    "PATCH /channels/{channel_id}/voice_state failed: HTTP {}",
                    response.status()
                )));
            }
            Ok(())
        }
    }

    /// Phase C of `plan-stoat-video-wasm.md` — route the UI camera-toggle through
    /// the inherent `StoatClient::start_video_capture` (defined in
    /// `crate::video_transport`) on WASM. Native builds fall through to the
    /// trait default (`NotSupported`) — the native `voice.rs` path is audio-only
    /// (see the A.5 architectural decision in the plan).
    #[cfg(target_arch = "wasm32")]
    async fn start_video_capture(&self, channel_id: &str) -> ClientResult<()> {
        // Delegate to the inherent method shipped in `crate::video_transport`.
        StoatClient::start_video_capture(self, channel_id).await
    }

    /// Phase C of `plan-stoat-video-wasm.md` — stop the WASM camera capture
    /// session via the inherent `StoatClient::stop_video_capture`. Native is a
    /// no-op (the trait default).
    #[cfg(target_arch = "wasm32")]
    async fn stop_video_capture(&self) -> ClientResult<()> {
        StoatClient::stop_video_capture(self);
        Ok(())
    }
}
