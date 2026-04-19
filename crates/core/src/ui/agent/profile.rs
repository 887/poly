//! Agent Profile section — shareable handshake card persisted via KV.

use crate::i18n::t;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

const KV_KEY: &str = "agent.profile.text";

/// Actions for the agent profile section.
pub enum AgentProfileAction {
    /// Save the profile text to KV.
    Save(String),
}

impl UiAction for AgentProfileAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::Save(_text) => {
                // Phase 5: persist via host KV bridge (poly_host kv_set).
                // For now this is a no-op; the signal already holds the value.
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(AgentProfileAction)]
#[component]
pub(super) fn AgentProfile() -> Element {
    let mut profile_text = use_signal(String::new);
    let mut saved = use_signal(|| false);

    // Load from KV on mount (no-op until host bridge is wired).
    use_effect(move || {
        let _ = KV_KEY; // placeholder: will call poly_host kv_get in phase 5
    });

    let on_save = move |_| {
        // Phase 5: write profile_text to host KV.
        let _ = profile_text.read().clone();
        saved.set(true);
    };

    let on_input = move |e: Event<FormData>| {
        profile_text.set(e.value());
        saved.set(false);
    };

    rsx! {
        div { class: "settings-section agent-profile-section",
            h2 { id: "agent-section-profile", "{t(\"agent-section-profile\")}" }
            p { class: "settings-description", "{t(\"agent-section-profile-desc\")}" }

            div { class: "agent-profile-editor",
                label { class: "agent-profile-label", r#for: "agent-profile-textarea",
                    "{t(\"agent-profile-textarea-label\")}"
                }
                textarea {
                    id: "agent-profile-textarea",
                    class: "agent-profile-textarea",
                    rows: "8",
                    value: "{profile_text.read()}",
                    oninput: on_input,
                }
                p { class: "agent-profile-visibility-note",
                    "{t(\"agent-profile-visibility-note\")}"
                }
                button {
                    class: "agent-profile-save-btn",
                    onclick: on_save,
                    if *saved.read() {
                        "✓ {t(\"agent-profile-save\")}"
                    } else {
                        "{t(\"agent-profile-save\")}"
                    }
                }
            }
        }
    }
}
