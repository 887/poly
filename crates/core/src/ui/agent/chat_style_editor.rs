//! Per-chat reply-style editor.
//!
//! Standalone component that renders a minimal form for the five style
//! fields (tone, formality, emoji_allowed, signature, extra_notes).
//!
//! The UI agent mounts this inside the right-sidebar chat panel; this file
//! only builds the form itself.
//!
//! Persistence: the [Save] button writes each field to the host KV store
//! under the key namespace `agent.style.<account_id>.<chat_id>.*`.  The
//! MCP side reads the same keys via the `get_chat_style` SQL table (which
//! the agent panel also seeds on save).  For the MVP we write directly to
//! KV — no loopback MCP call needed because `crate::STORAGE` is already
//! available on the WASM side.

use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

// ─── KV key helpers ───────────────────────────────────────────────────────────

fn kv_key(account_id: &str, chat_id: &str, field: &str) -> String {
    format!("agent.style.{account_id}.{chat_id}.{field}")
}

// ─── Component ────────────────────────────────────────────────────────────────

/// Reply-style editor for a single chat.
///
/// Props:
/// - `account_id` — Poly account ID this chat belongs to.
/// - `chat_id`    — The channel / DM / room ID.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub fn ChatStyleEditor(account_id: String, chat_id: String) -> Element {
    let mut tone         = use_signal(String::new);
    let mut formality    = use_signal(|| "neutral".to_string());
    let mut emoji        = use_signal(|| true);
    let mut signature    = use_signal(String::new);
    let mut extra_notes  = use_signal(String::new);
    let mut saved        = use_signal(|| false);

    // Load existing values from KV on first render.
    let acc = account_id.clone();
    let cid = chat_id.clone();
    use_effect(move || {
        let acc = acc.clone();
        let cid = cid.clone();
        spawn(async move {
            if let Some(storage) = crate::STORAGE.get() {
                if let Ok(Some(v)) = storage.get(&kv_key(&acc, &cid, "tone")).await {
                    if let Some(s) = v.as_str() { tone.set(s.to_string()); }
                }
                if let Ok(Some(v)) = storage.get(&kv_key(&acc, &cid, "formality")).await {
                    if let Some(s) = v.as_str() { formality.set(s.to_string()); }
                }
                if let Ok(Some(v)) = storage.get(&kv_key(&acc, &cid, "emoji_allowed")).await {
                    if let Some(b) = v.as_bool() { emoji.set(b); }
                }
                if let Ok(Some(v)) = storage.get(&kv_key(&acc, &cid, "signature")).await {
                    if let Some(s) = v.as_str() { signature.set(s.to_string()); }
                }
                if let Ok(Some(v)) = storage.get(&kv_key(&acc, &cid, "extra_notes")).await {
                    if let Some(s) = v.as_str() { extra_notes.set(s.to_string()); }
                }
            }
        });
    });

    let acc_save = account_id.clone();
    let cid_save = chat_id.clone();

    rsx! {
        div { class: "chat-style-editor",
            h3 { class: "settings-subsection-title", "{t(\"agent-style-title\")}" }

            // Tone dropdown
            div { class: "settings-toggle-row",
                label { class: "settings-toggle-label", "{t(\"agent-style-tone\")}" }
                select {
                    class: "settings-select",
                    value: "{tone.read()}",
                    onchange: move |e| tone.set(e.value()),
                    option { value: "", "—" }
                    option { value: "casual",       "{t(\"agent-style-tone-casual\")}" }
                    option { value: "professional", "{t(\"agent-style-tone-professional\")}" }
                    option { value: "snarky",       "{t(\"agent-style-tone-snarky\")}" }
                    option { value: "warm",         "{t(\"agent-style-tone-warm\")}" }
                    option { value: "direct",       "{t(\"agent-style-tone-direct\")}" }
                }
            }

            // Formality radio group
            div { class: "settings-toggle-row settings-radio-row",
                label { class: "settings-toggle-label", "{t(\"agent-style-formality\")}" }
                div { class: "settings-radio-group",
                    for (val, key) in [
                        ("tu",      "agent-style-formality-tu"),
                        ("vous",    "agent-style-formality-vous"),
                        ("neutral", "agent-style-formality-neutral"),
                    ] {
                        label { class: "settings-radio-label",
                            input {
                                r#type: "radio",
                                name: "chat-style-formality",
                                value: "{val}",
                                checked: *formality.read() == val,
                                onchange: move |_| formality.set(val.to_string()),
                            }
                            "{t(key)}"
                        }
                    }
                }
            }

            // Emoji toggle
            div { class: "settings-toggle-row",
                label { class: "settings-toggle-label", "{t(\"agent-style-emoji\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *emoji.read(),
                        onchange: move |e| emoji.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }

            // Signature textarea
            div { class: "settings-field",
                label { class: "settings-label", "{t(\"agent-style-signature\")}" }
                textarea {
                    class: "settings-textarea settings-textarea-sm",
                    rows: "2",
                    value: "{signature.read()}",
                    oninput: move |e| signature.set(e.value()),
                }
            }

            // Extra notes textarea
            div { class: "settings-field",
                label { class: "settings-label", "{t(\"agent-style-extra-notes\")}" }
                textarea {
                    class: "settings-textarea",
                    rows: "3",
                    value: "{extra_notes.read()}",
                    oninput: move |e| extra_notes.set(e.value()),
                }
            }

            // Save button
            button {
                class: "btn btn-primary btn-sm",
                onclick: move |_| {
                    let acc  = acc_save.clone();
                    let cid  = cid_save.clone();
                    let t_v  = tone.read().clone();
                    let f_v  = formality.read().clone();
                    let e_v  = *emoji.read();
                    let s_v  = signature.read().clone();
                    let n_v  = extra_notes.read().clone();
                    saved.set(false);
                    spawn(async move {
                        if let Some(storage) = crate::STORAGE.get() {
                            let _ = storage.set(&kv_key(&acc, &cid, "tone"),         serde_json::json!(t_v)).await;
                            let _ = storage.set(&kv_key(&acc, &cid, "formality"),    serde_json::json!(f_v)).await;
                            let _ = storage.set(&kv_key(&acc, &cid, "emoji_allowed"),serde_json::json!(e_v)).await;
                            let _ = storage.set(&kv_key(&acc, &cid, "signature"),    serde_json::json!(s_v)).await;
                            let _ = storage.set(&kv_key(&acc, &cid, "extra_notes"),  serde_json::json!(n_v)).await;
                        }
                        saved.set(true);
                    });
                },
                "{t(\"agent-style-save\")}"
            }

            if *saved.read() {
                span { class: "settings-save-confirmation", "✓" }
            }
        }
    }
}
