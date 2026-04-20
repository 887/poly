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
            Self::Save(text) => {
                spawn(async move {
                    if let Some(storage) = crate::STORAGE.get() {
                        if let Err(e) = storage.set(KV_KEY, serde_json::json!(text)).await {
                            tracing::warn!("Failed to persist agent.profile.text: {e}");
                        }
                    }
                });
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

    // Load persisted profile text from KV on mount.
    use_future(move || async move {
        let Some(storage) = crate::STORAGE.get() else { return };
        if let Ok(Some(v)) = storage.get(KV_KEY).await {
            if let Some(s) = v.as_str() {
                profile_text.set(s.to_string());
            }
        }
    });

    let on_save = move |_| {
        let text = profile_text.read().clone();
        spawn(async move {
            if let Some(storage) = crate::STORAGE.get() {
                if let Err(e) = storage.set(KV_KEY, serde_json::json!(text)).await {
                    tracing::warn!("Failed to persist agent.profile.text: {e}");
                }
            }
        });
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
