//! Extracted from lib.rs as part of SOLID B.1 split.
//!
//! Pure structural move — no behaviour change.

use super::super::DiscordClient;
use async_trait::async_trait;
use poly_client::{ClientResult, VoiceParticipant};


#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::VoiceTransportBackend for DiscordClient {
    async fn get_voice_participants(&self, channel_id: &str) -> ClientResult<Vec<VoiceParticipant>> {
        #[cfg(feature = "gateway")]
        {
            let states: tokio::sync::RwLockReadGuard<'_, std::collections::HashMap<String, Vec<VoiceParticipant>>> = self.voice_states.read().await;
            return Ok(states.get(channel_id).cloned().unwrap_or_default());
        }
        #[cfg(not(feature = "gateway"))]
        {
            let _ = channel_id;
            Ok(vec![])
        }
    }

    /// D.2 / D.5 — Initiate a DM call via the Discord gateway op 13.
    async fn start_dm_call_transport(&self, dm_channel_id: &str) -> ClientResult<()> {
        #[cfg(feature = "gateway")]
        {
            self.start_direct_call(dm_channel_id)?;
        }
        #[cfg(not(feature = "gateway"))]
        {
            let _ = dm_channel_id;
        }
        Ok(())
    }

    /// C.1 — Signal the gateway that the local user is joining a voice channel.
    ///
    /// - `gateway` (native, not wasm32): sends op 4 Voice State Update on the
    ///   main gateway back-channel. Requires `event_stream()` to have been called
    ///   first (which opens the WS and sets `gateway_tx`).
    /// - `voice-bridge` (wasm32): initialises a `DiscordVoiceBridgeClient` and
    ///   drives the full Discord voice protocol over generic host-bridge primitives
    ///   (`/host/udp/*`, `/host/codec/opus/*`, `/host/aead/*`). The WS endpoint
    ///   and credentials are expected to be cached on the client by the time this
    ///   is called (set via `VOICE_SERVER_UPDATE` / `VOICE_STATE_UPDATE` events on
    ///   whatever event path the wasm32 shell uses). Until full wiring of that
    ///   credential path ships, the call proceeds with empty credential strings,
    ///   matching the existing `finish_handshake` stub behaviour in `voice_bridge.rs`.
    async fn join_voice_channel_transport(
        &self,
        server_id: &str,
        channel_id: &str,
    ) -> ClientResult<()> {
        #[cfg(all(feature = "gateway", not(target_arch = "wasm32")))]
        {
            self.set_self_mute(server_id, Some(channel_id), false, false)?;
        }
        #[cfg(all(feature = "voice-bridge", target_arch = "wasm32"))]
        {
            tracing::info!(
                target: "poly_discord::voice_bridge",
                server_id,
                channel_id,
                "join_voice_channel_transport: dispatching via DiscordVoiceBridgeClient"
            );

            // gateway-bridge: send op 4 Voice State Update so Discord dispatches
            // VOICE_STATE_UPDATE + VOICE_SERVER_UPDATE back to us.
            #[cfg(feature = "gateway-bridge")]
            {
                let op4 = serde_json::json!({
                    "op": 4,
                    "d": {
                        "guild_id": server_id,
                        "channel_id": channel_id,
                        "self_mute": false,
                        "self_deaf": false,
                    }
                });
                if let Ok(guard) = self.gateway_bridge_tx.lock() {
                    if let Some(tx) = guard.as_ref() {
                        let _ = tx.send(op4.to_string());
                        tracing::info!(
                            target: "poly_discord::gateway_bridge",
                            server_id,
                            channel_id,
                            "join_voice_channel_transport: sent op4 via gateway-bridge"
                        );
                    } else {
                        tracing::warn!(
                            target: "poly_discord::gateway_bridge",
                            "join_voice_channel_transport: gateway-bridge not yet connected \
                             (event_stream not called or connection pending)"
                        );
                    }
                }
            }

            // Read voice credentials — populated by the gateway-bridge loop from
            // VOICE_STATE_UPDATE + VOICE_SERVER_UPDATE once Discord processes op 4.
            // These arrive asynchronously (~3–50 ms after op 4 is sent), so poll
            // with wait_for_voice_creds instead of taking a single-shot snapshot.
            // When gateway-bridge is not enabled, fall back to empty strings (the
            // finish_handshake stub in voice_bridge.rs surfaces a clear error).
            #[cfg(feature = "gateway-bridge")]
            let (ws_endpoint, ws_token, ws_session_id) = {
                match self.wait_for_voice_creds(1000).await {
                    Some(creds) => creds,
                    None => {
                        tracing::warn!(
                            target: "poly_discord::gateway_bridge",
                            "join_voice_channel_transport: timed out waiting for \
                             VOICE_SERVER_UPDATE creds (endpoint/token/session_id) — \
                             proceeding with empty strings; connect_voice will fail"
                        );
                        (String::new(), String::new(), String::new())
                    }
                }
            };
            #[cfg(not(feature = "gateway-bridge"))]
            let (ws_endpoint, ws_token, ws_session_id) =
                (String::new(), String::new(), String::new());

            let account_id = self.account_id();
            let mut guard = self.voice_bridge_client.lock().await;
            if guard.is_none() {
                *guard = Some(voice_bridge::DiscordVoiceBridgeClient::new(account_id));
            }
            let client = guard.as_ref().expect("just initialised above");
            let dummy_audio = poly_audio_backend::fake_backend::FakeAudioBackend::new();
            if let Err(e) = client
                .connect_voice(&ws_endpoint, &ws_token, &ws_session_id, Some(server_id), &dummy_audio, None)
                .await
            {
                tracing::warn!(
                    target: "poly_discord::voice_bridge",
                    error = %e,
                    "join_voice_channel_transport: connect_voice returned error"
                );
            }
        }
        #[cfg(not(any(
            all(feature = "gateway", not(target_arch = "wasm32")),
            all(feature = "voice-bridge", target_arch = "wasm32"),
        )))]
        {
            let _ = (server_id, channel_id);
        }
        Ok(())
    }

    /// C.5 — Toggle the local user's mute / deafen state on the Discord gateway.
    ///
    /// - `gateway` (native, not wasm32): sends op 4 Voice State Update with the
    ///   updated flags on the main gateway back-channel.
    /// - `voice-bridge` (wasm32): delegates to `DiscordVoiceBridgeClient::set_self_mute`
    ///   which returns an error when no voice session is active (guards against
    ///   a stale mute toggle before `join_voice_channel_transport` completes).
    async fn set_voice_mute(
        &self,
        server_id: &str,
        channel_id: &str,
        self_mute: bool,
        self_deaf: bool,
    ) -> ClientResult<()> {
        #[cfg(all(feature = "gateway", not(target_arch = "wasm32")))]
        {
            self.set_self_mute(server_id, Some(channel_id), self_mute, self_deaf)?;
        }
        #[cfg(all(feature = "voice-bridge", target_arch = "wasm32"))]
        {
            tracing::info!(
                target: "poly_discord::voice_bridge",
                server_id,
                channel_id,
                self_mute,
                self_deaf,
                "set_voice_mute: dispatching via DiscordVoiceBridgeClient"
            );
            let guard = self.voice_bridge_client.lock().await;
            if let Some(client) = guard.as_ref() {
                if let Err(e) = client
                    .set_self_mute(server_id, Some(channel_id), self_mute, self_deaf)
                    .await
                {
                    tracing::warn!(
                        target: "poly_discord::voice_bridge",
                        error = %e,
                        "set_voice_mute: bridge returned error (no active session?)"
                    );
                }
            }
        }
        #[cfg(not(any(
            all(feature = "gateway", not(target_arch = "wasm32")),
            all(feature = "voice-bridge", target_arch = "wasm32"),
        )))]
        {
            let _ = (server_id, channel_id, self_mute, self_deaf);
        }
        Ok(())
    }
}
