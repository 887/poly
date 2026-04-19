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
                // TODO: persist via host KV bridge (key: agent.profile.text) in Phase 5.
                // Signal already holds the value for local state.
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

    // Load from KV on mount (no-op until host bridge is wired in Phase 5).
    use_effect(move || {
        let _ = KV_KEY; // placeholder: will call poly_host kv_get in phase 5
    });

    let on_save = move |_| {
        // TODO: write profile_text to host KV in Phase 5.
        let _ = profile_text.read().clone();
        saved.set(true);
    };

    let on_input = move |e: Event<FormData>| {
        profile_text.set(e.value());
        saved.set(false);
    };

    rsx! {
        div { class: "settings-section",
            h2 { id: "agent-section-profile", "{t(\"agent-section-profile\")}" }
            p { class: "settings-description", "{t(\"agent-section-profile-desc\")}" }

            label { class: "settings-label", r#for: "agent-profile-textarea",
                "{t(\"agent-profile-textarea-label\")}"
            }
            textarea {
                id: "agent-profile-textarea",
                class: "css-editor",
                rows: "8",
                value: "{profile_text.read()}",
                oninput: on_input,
            }
            p { class: "settings-description", "{t(\"agent-profile-visibility-note\")}" }
            button {
                class: "btn btn-secondary",
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
