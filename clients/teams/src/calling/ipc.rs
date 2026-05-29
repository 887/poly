//! IPC wire shapes for the Rust ↔ JS calling bridge — Phase C.2 of
//! `docs/plans/plan-teams-calling.md`.
//!
//! The Phase C bridge runs the official `@azure/communication-calling`
//! JS SDK inside a hidden WebView frame and shuttles
//! [`CallingCommand`] frames out to JS, [`CallingEvent`] frames back
//! in. This module defines the **stable wire types** for that bridge —
//! the JS file (`apps/desktop-electron-web/src/calling-bridge.ts` or
//! equivalent) will mirror these types in TypeScript and serialise via
//! `postMessage`. Until that JS file lands the wire stays unused in
//! production code, but the types are exercised by unit tests so they
//! survive refactors with their shape intact.
//!
//! ## Serialization
//!
//! Both enums use serde's internally-tagged representation with a
//! `"kind"` discriminant, so the JS side can `switch (msg.kind)` to
//! route. All other fields are `camelCase` to match TS idiom.
//!
//! ## JS-side mirror (documentation only — no .ts file yet)
//!
//! ```ts
//! // Rust → JS: outbound commands
//! type CallingCommand =
//!   | { kind: "init"; acsEndpoint: string; acsUserId: string; token: string }
//!   | { kind: "connectVoice"; channelId: string }
//!   | { kind: "startDirectCall"; chatId: string }
//!   | { kind: "disconnect"; callId: string }
//!   | { kind: "setMute"; callId: string; muted: boolean }
//!   | { kind: "startVideo"; callId: string }
//!   | { kind: "stopVideo"; callId: string }
//!   | { kind: "shareScreen"; callId: string }
//!   | { kind: "stopScreenShare"; callId: string }
//!   | { kind: "hold"; callId: string }
//!   | { kind: "resume"; callId: string }
//!   | { kind: "acceptIncoming"; callId: string }
//!   | { kind: "rejectIncoming"; callId: string }
//!   | { kind: "queryParticipants"; channelId: string };
//!
//! // JS → Rust: inbound events
//! type CallingEvent =
//!   | { kind: "ready" }
//!   | { kind: "callConnected"; callId: string }
//!   | { kind: "callDisconnected"; callId: string; reason?: string }
//!   | { kind: "stateChanged"; callId: string; state: string }
//!   | { kind: "incomingCall"; callId: string; from: string }
//!   | { kind: "participantsChanged"; channelId: string; participants: string[] }
//!   | { kind: "remoteMute"; callId: string; userId: string; muted: boolean }
//!   | { kind: "videoStreamAvailable"; callId: string; userId: string; streamId: string }
//!   | { kind: "screenShareStarted"; callId: string; userId: string }
//!   | { kind: "screenShareStopped"; callId: string; userId: string }
//!   | { kind: "error"; message: string; recoverable: boolean };
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::types::CallingError;

/// Outbound IPC frame — Rust → JS bridge.
///
/// Internally-tagged: serialises as
/// `{"kind":"connectVoice","channelId":"..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CallingCommand {
    /// One-shot bootstrap. Sent after the WebView reports `ready`.
    /// Hands the JS side the ACS endpoint, the user identity, and a
    /// fresh access token. JS uses this to construct a `CallClient` +
    /// `CallAgent` and start observing events.
    Init {
        acs_endpoint: String,
        acs_user_id: String,
        token: String,
    },

    /// Join a Teams channel voice meeting.
    ConnectVoice { channel_id: String },

    /// Place a 1:1 / group direct call to a DM.
    StartDirectCall { chat_id: String },

    /// Hang up the named call.
    Disconnect { call_id: String },

    /// Granular mute (audio-only).
    SetMute { call_id: String, muted: bool },

    /// Start sending local video.
    StartVideo { call_id: String },

    /// Stop sending local video.
    StopVideo { call_id: String },

    /// Begin screen-share.
    ShareScreen { call_id: String },

    /// Stop screen-share.
    StopScreenShare { call_id: String },

    /// Put the call on hold.
    Hold { call_id: String },

    /// Resume a held call.
    Resume { call_id: String },

    /// Accept an inbound call.
    AcceptIncoming { call_id: String },

    /// Reject an inbound call.
    RejectIncoming { call_id: String },

    /// Ask JS for the current participant list of a channel meeting.
    /// Response arrives asynchronously as
    /// [`CallingEvent::ParticipantsChanged`].
    QueryParticipants { channel_id: String },
}

/// Inbound IPC frame — JS → Rust bridge.
///
/// Internally-tagged: serialises as
/// `{"kind":"callConnected","callId":"..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CallingEvent {
    /// Bridge is up; safe to send [`CallingCommand::Init`].
    Ready,

    /// A new call reached the `Connected` state.
    CallConnected { call_id: String },

    /// A call was torn down (either side).
    CallDisconnected {
        call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    /// Call lifecycle state moved to the named state. State is the
    /// string form of [`super::types::CallState`] so the JS side can
    /// emit the SDK's own string and let Rust map it.
    StateChanged { call_id: String, state: String },

    /// An incoming call rang locally — surface to UI for accept/reject.
    IncomingCall { call_id: String, from: String },

    /// Channel participant list updated. List is opaque user-id strings;
    /// Rust looks them up against `clients/teams::types::User` for
    /// display-name + avatar.
    ParticipantsChanged {
        channel_id: String,
        participants: Vec<String>,
    },

    /// A remote participant changed their mute state.
    RemoteMute {
        call_id: String,
        user_id: String,
        muted: bool,
    },

    /// A new remote video stream is available to subscribe to.
    VideoStreamAvailable {
        call_id: String,
        user_id: String,
        stream_id: String,
    },

    /// A remote participant started screen-sharing.
    ScreenShareStarted { call_id: String, user_id: String },

    /// A remote participant stopped screen-sharing.
    ScreenShareStopped { call_id: String, user_id: String },

    /// JS-side error. `recoverable=true` means the bridge survives;
    /// `false` means the JS half is dead and needs re-init.
    Error { message: String, recoverable: bool },
}

/// Transport handle that shuttles [`CallingCommand`] out and
/// [`CallingEvent`] in.
///
/// Object-safe so [`super::client::WebViewBridgeCallingClient`] can
/// store one as `Arc<dyn CallingTransport>` without leaking generics.
/// The real impl will wrap a `postMessage` channel + a JS event
/// listener; tests use [`MockCallingTransport`].
#[async_trait]
pub trait CallingTransport: Send + Sync {
    /// Send one command to the JS side.
    async fn send(&self, cmd: CallingCommand) -> Result<(), CallingError>;

    /// Receive the next event from the JS side. Returns `None` when
    /// the transport is closed.
    async fn recv(&self) -> Option<CallingEvent>;
}

/// In-memory test transport.
///
/// - `send(cmd)` pushes onto an internal sent-log.
/// - `recv()` pops the next queued event (or returns `None`).
/// - `inject_event(ev)` lets a test enqueue a fake JS response.
/// - `sent_commands()` snapshots the log for assertions.
///
/// All methods take `&self` (interior mutability via `Mutex`) so the
/// transport can live behind an `Arc` in a `WebViewBridgeCallingClient`.
#[derive(Default)]
pub struct MockCallingTransport {
    sent: std::sync::Mutex<Vec<CallingCommand>>,
    inbox: std::sync::Mutex<std::collections::VecDeque<CallingEvent>>,
}

impl std::fmt::Debug for MockCallingTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockCallingTransport").finish()
    }
}

impl MockCallingTransport {
    /// Construct a new empty mock.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue an event for the next [`Self::recv`] call.
    ///
    /// Mutex poisoning is recovered transparently (`unwrap_or_else`
    /// into the inner value) — a poisoned mock-transport mutex would
    /// only happen if a test panicked mid-mutation, in which case the
    /// downstream queue state is still readable / writable.
    pub fn inject_event(&self, ev: CallingEvent) {
        let mut inbox = self.inbox.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        inbox.push_back(ev);
    }

    /// Snapshot the sent-command log. Mutex poisoning recovered as in
    /// [`Self::inject_event`].
    #[must_use]
    pub fn sent_commands(&self) -> Vec<CallingCommand> {
        let sent = self.sent.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        sent.clone()
    }
}

#[async_trait]
impl CallingTransport for MockCallingTransport {
    async fn send(&self, cmd: CallingCommand) -> Result<(), CallingError> {
        self.sent.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(cmd);
        Ok(())
    }

    async fn recv(&self) -> Option<CallingEvent> {
        self.inbox.lock().unwrap_or_else(std::sync::PoisonError::into_inner).pop_front()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn command_init_round_trips() {
        let cmd = CallingCommand::Init {
            acs_endpoint: "https://x".into(),
            acs_user_id: "8:acs:y".into(),
            token: "jwt".into(),
        };
        let s = serde_json::to_string(&cmd).unwrap();
        assert!(s.contains("\"kind\":\"init\""));
        assert!(s.contains("\"acsEndpoint\":\"https://x\""));
        let back: CallingCommand = serde_json::from_str(&s).unwrap();
        assert_eq!(back, cmd);
    }

    #[test]
    fn command_connect_voice_round_trips() {
        let cmd = CallingCommand::ConnectVoice {
            channel_id: "ch1".into(),
        };
        let s = serde_json::to_string(&cmd).unwrap();
        assert!(s.contains("\"kind\":\"connectVoice\""));
        assert!(s.contains("\"channelId\":\"ch1\""));
        let back: CallingCommand = serde_json::from_str(&s).unwrap();
        assert_eq!(back, cmd);
    }

    #[test]
    fn command_set_mute_round_trips() {
        let cmd = CallingCommand::SetMute {
            call_id: "c1".into(),
            muted: true,
        };
        let s = serde_json::to_string(&cmd).unwrap();
        assert!(s.contains("\"kind\":\"setMute\""));
        assert!(s.contains("\"muted\":true"));
        let back: CallingCommand = serde_json::from_str(&s).unwrap();
        assert_eq!(back, cmd);
    }

    #[test]
    fn command_video_screen_hold_resume_variants_distinct() {
        for c in [
            CallingCommand::StartVideo {
                call_id: "c".into(),
            },
            CallingCommand::StopVideo {
                call_id: "c".into(),
            },
            CallingCommand::ShareScreen {
                call_id: "c".into(),
            },
            CallingCommand::StopScreenShare {
                call_id: "c".into(),
            },
            CallingCommand::Hold {
                call_id: "c".into(),
            },
            CallingCommand::Resume {
                call_id: "c".into(),
            },
        ] {
            let s = serde_json::to_string(&c).unwrap();
            let back: CallingCommand = serde_json::from_str(&s).unwrap();
            assert_eq!(back, c);
        }
    }

    #[test]
    fn event_ready_round_trips() {
        let ev = CallingEvent::Ready;
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(s, "{\"kind\":\"ready\"}");
        let back: CallingEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(back, ev);
    }

    #[test]
    fn event_call_disconnected_omits_none_reason() {
        let ev = CallingEvent::CallDisconnected {
            call_id: "c".into(),
            reason: None,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(!s.contains("reason"), "None reason should be omitted: {s}");
    }

    #[test]
    fn event_call_disconnected_with_reason_round_trips() {
        let ev = CallingEvent::CallDisconnected {
            call_id: "c".into(),
            reason: Some("hung-up".into()),
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("\"reason\":\"hung-up\""));
        let back: CallingEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(back, ev);
    }

    #[test]
    fn event_participants_changed_round_trips() {
        let ev = CallingEvent::ParticipantsChanged {
            channel_id: "ch".into(),
            participants: vec!["u1".into(), "u2".into()],
        };
        let s = serde_json::to_string(&ev).unwrap();
        let back: CallingEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(back, ev);
    }

    #[test]
    fn event_error_round_trips() {
        let ev = CallingEvent::Error {
            message: "boom".into(),
            recoverable: false,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("\"recoverable\":false"));
        let back: CallingEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(back, ev);
    }

    // ── MockCallingTransport ──────────────────────────────────────────

    #[tokio::test]
    async fn mock_transport_send_records_command() {
        let t = MockCallingTransport::new();
        t.send(CallingCommand::ConnectVoice {
            channel_id: "ch1".into(),
        })
        .await
        .unwrap();
        let sent = t.sent_commands();
        assert_eq!(sent.len(), 1);
        assert!(matches!(&sent[0], CallingCommand::ConnectVoice { .. }));
    }

    #[tokio::test]
    async fn mock_transport_recv_empty_returns_none() {
        let t = MockCallingTransport::new();
        assert!(t.recv().await.is_none());
    }

    #[tokio::test]
    async fn mock_transport_recv_yields_injected_events_in_order() {
        let t = MockCallingTransport::new();
        t.inject_event(CallingEvent::Ready);
        t.inject_event(CallingEvent::CallConnected {
            call_id: "c1".into(),
        });
        assert!(matches!(t.recv().await, Some(CallingEvent::Ready)));
        assert!(matches!(
            t.recv().await,
            Some(CallingEvent::CallConnected { .. })
        ));
        assert!(t.recv().await.is_none());
    }

    #[tokio::test]
    async fn mock_transport_handles_init_command() {
        let t = MockCallingTransport::new();
        t.send(CallingCommand::Init {
            acs_endpoint: "https://x".into(),
            acs_user_id: "8:acs:y".into(),
            token: "jwt".into(),
        })
        .await
        .unwrap();
        let sent = t.sent_commands();
        assert_eq!(sent.len(), 1);
        match &sent[0] {
            CallingCommand::Init { acs_endpoint, .. } => {
                assert_eq!(acs_endpoint, "https://x");
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }
}
