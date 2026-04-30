//! Typed-confirm modals for destructive persona actions.
//!
//! G.5 — Typed-confirm modal on first outbound-mode enable.
//! H.6 — "Forget all persona memory" + "Delete persona" typed-confirm flows.
//!
//! ## Pattern
//! Each modal renders a text input; the action button stays disabled until the
//! user types the expected slug/keyword exactly.  Matching is case-sensitive.
//!
//! ## Reactive hygiene
//! All signals are single-component-scoped local `Signal<T>` — no cross-
//! component subscribers, so `.set()` is safe without BatchedSignal.

use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

// ─── ConfirmOutboundModal (G.5) ───────────────────────────────────────────────

/// Renders a typed-confirm dialog that must be dismissed before a persona can
/// be switched to `outbound-allowlisted` proactivity for the first time.
///
/// The user must type the persona slug exactly to unlock the confirm button.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ConfirmOutboundModal(
    persona_slug: String,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut typed = use_signal(String::new);
    let slug_copy = persona_slug.clone();
    let matches = typed.read().as_str() == slug_copy.as_str();

    rsx! {
        div { class: "persona-modal-overlay",
            onclick: move |_| on_cancel.call(()),
            div { class: "persona-modal persona-confirm-modal",
                onclick: move |evt| evt.stop_propagation(),

                div { class: "persona-modal-header",
                    h3 { class: "persona-modal-title persona-danger-title",
                        {t("persona-confirm-outbound-title")}
                    }
                }
                div { class: "persona-modal-body",
                    p { class: "persona-confirm-description",
                        {t("persona-confirm-outbound-description")}
                    }
                    ul { class: "persona-confirm-warnings",
                        li { {t("persona-confirm-outbound-warn1")} }
                        li { {t("persona-confirm-outbound-warn2")} }
                        li { {t("persona-confirm-outbound-warn3")} }
                    }
                    div { class: "settings-field",
                        label { class: "settings-label",
                            {format!("{} \"{}\":", t("persona-confirm-type-slug"), persona_slug)}
                        }
                        input {
                            r#type: "text",
                            class: "settings-input persona-confirm-input",
                            placeholder: persona_slug.clone(),
                            value: "{typed.read()}",
                            oninput: move |e| typed.set(e.value()),
                        }
                    }
                }
                div { class: "persona-modal-footer",
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| on_cancel.call(()),
                        {t("persona-action-cancel")}
                    }
                    button {
                        class: "btn btn-danger",
                        disabled: !matches,
                        onclick: {
                            let on_confirm = on_confirm.clone();
                            move |_| {
                                if matches {
                                    on_confirm.call(());
                                }
                            }
                        },
                        {t("persona-confirm-outbound-enable")}
                    }
                }
            }
        }
    }
}

// ─── ConfirmForgetMemoryModal (H.6a) ─────────────────────────────────────────

/// Typed-confirm for "Forget all persona memory" — irreversibly deletes all
/// facts for this persona.  User must type the slug to confirm.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ConfirmForgetMemoryModal(
    persona_slug: String,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut typed = use_signal(String::new);
    let slug_copy = persona_slug.clone();
    let matches = typed.read().as_str() == slug_copy.as_str();

    rsx! {
        div { class: "persona-modal-overlay",
            onclick: move |_| on_cancel.call(()),
            div { class: "persona-modal persona-confirm-modal",
                onclick: move |evt| evt.stop_propagation(),

                div { class: "persona-modal-header",
                    h3 { class: "persona-modal-title persona-danger-title",
                        {t("persona-confirm-forget-title")}
                    }
                }
                div { class: "persona-modal-body",
                    p { class: "persona-confirm-description",
                        {t("persona-confirm-forget-description")}
                    }
                    div { class: "settings-field",
                        label { class: "settings-label",
                            {format!("{} \"{}\":", t("persona-confirm-type-slug"), persona_slug)}
                        }
                        input {
                            r#type: "text",
                            class: "settings-input persona-confirm-input",
                            placeholder: persona_slug.clone(),
                            value: "{typed.read()}",
                            oninput: move |e| typed.set(e.value()),
                        }
                    }
                }
                div { class: "persona-modal-footer",
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| on_cancel.call(()),
                        {t("persona-action-cancel")}
                    }
                    button {
                        class: "btn btn-danger",
                        disabled: !matches,
                        onclick: {
                            let on_confirm = on_confirm.clone();
                            move |_| {
                                if matches {
                                    on_confirm.call(());
                                }
                            }
                        },
                        {t("persona-confirm-forget-execute")}
                    }
                }
            }
        }
    }
}

// ─── ConfirmDeletePersonaModal (H.6b) ─────────────────────────────────────────

/// Typed-confirm for "Delete persona" — irreversibly deletes the persona and
/// all its associated data (cascades in the DB).  User must type the slug.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ConfirmDeletePersonaModal(
    persona_slug: String,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut typed = use_signal(String::new);
    let slug_copy = persona_slug.clone();
    let matches = typed.read().as_str() == slug_copy.as_str();

    rsx! {
        div { class: "persona-modal-overlay",
            onclick: move |_| on_cancel.call(()),
            div { class: "persona-modal persona-confirm-modal",
                onclick: move |evt| evt.stop_propagation(),

                div { class: "persona-modal-header",
                    h3 { class: "persona-modal-title persona-danger-title",
                        {t("persona-confirm-delete-title")}
                    }
                }
                div { class: "persona-modal-body",
                    p { class: "persona-confirm-description",
                        {t("persona-confirm-delete-description")}
                    }
                    div { class: "settings-field",
                        label { class: "settings-label",
                            {format!("{} \"{}\":", t("persona-confirm-type-slug"), persona_slug)}
                        }
                        input {
                            r#type: "text",
                            class: "settings-input persona-confirm-input",
                            placeholder: persona_slug.clone(),
                            value: "{typed.read()}",
                            oninput: move |e| typed.set(e.value()),
                        }
                    }
                }
                div { class: "persona-modal-footer",
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| on_cancel.call(()),
                        {t("persona-action-cancel")}
                    }
                    button {
                        class: "btn btn-danger",
                        disabled: !matches,
                        onclick: {
                            let on_confirm = on_confirm.clone();
                            move |_| {
                                if matches {
                                    on_confirm.call(());
                                }
                            }
                        },
                        {t("persona-confirm-delete-execute")}
                    }
                }
            }
        }
    }
}
