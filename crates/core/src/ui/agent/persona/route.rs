//! PersonaManagementRoute — full-page persona management UI at `/agent/personas`.
//!
//! Lists all personas in a full-page layout with a prominent "Create" button.
//! Opens PersonaEditModal inline.
//!
//! Phase E: the full-page route also wires a local TalkSession signal so the
//! "Talk to" button works when accessed from the management route directly.

use super::list_panel::PersonaListPanel;
use super::talk_to_overlay::{PersonaTalkToOverlay, TalkSession};
use super::types::PersonaSummary;
use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Generate a simple session ID for the management route.
fn route_session_id() -> String {
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
        let r = (js_sys::Math::random() * u64::MAX as f64) as u64;
        format!("{r:016x}")
    }
}

/// Full-page persona management component.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn PersonaManagementRoute() -> Element {
    let mut talk_session: Signal<Option<TalkSession>> = use_signal(|| None);
    // Peek to avoid subscribing the whole route to talk_session (hang class #7).
    let current_talk = talk_session.peek().clone();

    rsx! {
        div { class: "persona-management-page",
            div { class: "special-page-header",
                h2 { class: "special-page-title", {t("persona-management-title")} }
            }
            div { class: "persona-management-body",
                p { class: "settings-description", {t("persona-management-desc")} }
                PersonaListPanel {
                    on_talk: move |summary: PersonaSummary| {
                        let session = TalkSession {
                            persona_slug: summary.slug.clone(),
                            persona_name: summary.name.clone(),
                            persona_avatar: summary.avatar_emoji.clone(),
                            session_id: route_session_id(),
                        };
                        talk_session.set(Some(session));
                    },
                }
            }
            if let Some(session) = current_talk {
                PersonaTalkToOverlay {
                    session,
                    on_close: move |_| talk_session.set(None),
                }
            }
        }
    }
}
