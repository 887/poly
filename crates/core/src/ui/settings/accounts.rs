//! Accounts settings section.
//!
//! Lists all active messenger accounts with their display name, backend badge,
//! and a gear icon linking to /:backend/:instance_id/:account_id/settings.
//!
//! The "Add Account" button navigates to the plugin-driven routable signup flow
//! at `/signup`, where the user picks a backend and completes a full-page form.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::ChatData;
use crate::ui::routes::Route;
use dioxus::prelude::*;

/// Derive a stable hsl color from an account ID string (same as search.rs).
fn account_color(account_id: &str) -> String {
    let hash: u32 = account_id.bytes().fold(5381_u32, |h, b| {
        h.wrapping_mul(33).wrapping_add(u32::from(b))
    });
    let hue = hash % 360;
    format!("hsl({hue}, 65%, 55%)")
}

/// Emoji icon for a backend slug.
fn backend_emoji(slug: &str) -> &'static str {
    match slug {
        "demo" => "🧪",
        "stoat" => "🦦",
        "matrix" => "🟩",
        "discord" => "🟣",
        "teams" => "🟦",
        "poly" => "🔷",
        _ => "💬",
    }
}

/// A single row in the accounts list showing account icon, name, backend, and settings gear.
#[rustfmt::skip]
#[component]
fn AccountRow(
    account_id: String,
    display_name: String,
    backend_slug: String,
    backend_label: String,
    instance_id: String,
    icon_color: String,
    avatar_url: Option<String>,
) -> Element {
    let icon_char: String = display_name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".to_string());
    let emoji = backend_emoji(&backend_slug);
    rsx! {
        div { class: "accounts-settings-row",
            // Avatar image if available; otherwise colored letter bubble
            if let Some(url) = avatar_url.as_ref() {
                img {
                    class: "accounts-settings-icon accounts-settings-icon--img",
                    src: "{url}",
                    alt: "{display_name}",
                }
            } else {
                div {
                    class: "accounts-settings-icon",
                    style: "background: {icon_color}",
                    "{icon_char}"
                }
            }
            // Name + backend label
            div { class: "accounts-settings-info",
                span { class: "accounts-settings-name", "{display_name}" }
                span { class: "accounts-settings-backend", "{emoji} {backend_label}" }
            }
            // Gear icon → account settings
            Link {
                to: Route::AccountSettingsRoute {
                    backend: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                },
                class: "accounts-settings-gear",
                title: "{t(\"settings-account-settings-link\")}",
                "⚙"
            }
        }
    }
}

/// Accounts settings section.
///
/// Lists active messenger accounts grouped by backend and provides
/// an "Add Account" button that navigates to the plugin-driven signup
/// flow at `/signup`.
#[rustfmt::skip]
#[component]
pub(super) fn AccountsSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let client_manager: Signal<ClientManager> = use_context();
    let _chat_data: Signal<ChatData> = use_context();

    let account_ids = client_manager.read().active_account_ids();

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-accounts\")}" }
            p { class: "settings-description", "{t(\"settings-accounts-description\")}" }

            if account_ids.is_empty() {
                p { class: "settings-empty-hint", "{t(\"settings-no-accounts\")}" }
            } else {
                div { class: "accounts-settings-list",
                    for account_id in &account_ids {
                        {
                            let aid = account_id.clone();
                            let cm = client_manager.read();
                            let session = cm.sessions.get(&aid);
                            let display_name = session
                                .map(|s| s.user.display_name.clone())
                                .unwrap_or_else(|| aid.clone());
                            let backend_slug = session
                                .map(|s| s.backend.slug().to_string())
                                .unwrap_or_else(|| "demo".to_string());
                            let backend_label = session
                                .map(|s| s.backend.display_name().to_string())
                                .unwrap_or_else(|| "Demo".to_string());
                            let instance_id = session
                                .map(|s| s.instance_id.clone())
                                .unwrap_or_else(|| "demo".to_string());
                            let icon_color = account_color(&aid);
                            let avatar_url = session
                                .and_then(|s| s.user.avatar_url.clone());
                            rsx! {
                                AccountRow {
                                    key: "{aid}",
                                    account_id: aid,
                                    display_name,
                                    backend_slug,
                                    backend_label,
                                    instance_id,
                                    icon_color,
                                    avatar_url,
                                }
                            }
                        }
                    }
                }
            }

            // Navigate to the plugin-driven, routable signup picker at /signup.
            button {
                class: "btn btn-primary",
                onclick: move |_| { let _ = navigator().push(Route::SignupPicker); },
                "{t(\"settings-add-account\")}"
            }
        }
    }
}
