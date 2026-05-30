//! PersonasSection — persona management UI rendered inline inside `AgentPage`.
//!
//! Lists all personas with a "+ New persona" button and wires the per-row
//! "Talk to" overlay. No outer page chrome — fits inside the agent settings
//! section stack (AgentPage's `AgentAllSections`).

use super::list_panel::PersonaListPanel;
use super::talk_to_overlay::{PersonaTalkToOverlay, TalkSession};
use super::types::PersonaSummary;
use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Generate a simple session ID for the talk-to overlay.
fn session_id() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{ts:x}")
    }
    #[cfg(target_arch = "wasm32")]
    {
        // lint-allow-unused: f64-to-u64 cast on bounded random value
        // (clamped 0..=u64::MAX before cast).
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions, clippy::cast_sign_loss, clippy::cast_precision_loss)]
        let r = (js_sys::Math::random() * u64::MAX as f64) as u64;
        format!("{r:016x}")
    }
}

/// Persona management section. Renders inline inside `AgentPage` — sits in
/// the same section stack as Integrations and Profile, so the agent sub-nav
/// (Integrations / Profile / Personas) stays visible.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn PersonasSection() -> Element {
    let mut talk_session: Signal<Option<TalkSession>> = use_signal(|| None);
    // Peek to avoid subscribing the whole section to talk_session (hang class #7).
    let current_talk = talk_session.peek().clone();

    rsx! {
        h2 { id: "agent-section-personas", "{t(\"persona-panel-title\")}" }
        p { class: "settings-description", "{t(\"persona-management-desc\")}" }
        PersonaListPanel {
            on_talk: move |summary: PersonaSummary| {
                let session = TalkSession {
                    persona_slug: summary.slug.clone(),
                    persona_name: summary.name.clone(),
                    persona_avatar: summary.avatar_emoji.clone(),
                    session_id: session_id(),
                };
                talk_session.set(Some(session));
            },
        }
        if let Some(session) = current_talk {
            PersonaTalkToOverlay {
                session,
                on_close: move |()| talk_session.set(None),
            }
        }
    }
}
