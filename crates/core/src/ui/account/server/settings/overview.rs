//! Server overview settings — icon and banner configuration.
//!
//! Allows users to customise the server icon and banner image URLs. Both
//! fields always render; writes are local-only overrides stored in
//! `AppSettings`. Backend-specific gating (previously: banner-only for some
//! slugs, checkbox-gated icon for Matrix/Teams) was removed in WP 3 — plugin-
//! declared `PerServer` settings sections take that role now.
//!
//! ## Components
//! - [`ServerOverviewSettings`] — root, always shows icon + banner panels
//! - [`IconPanel`] — icon URL input + preview
//! - [`BannerPanel`] — banner URL input + preview

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::ChatData;
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::toast::{ToastMessage, push_toast};
use dioxus::prelude::*;
use poly_client::ToastTone;
use poly_ui_macros::{context_menu, ui_action};

pub enum IconPanelAction {
    SetUrl(String),
    Save,
}

impl UiAction for IconPanelAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetUrl(_) => todo!("phase-E: update icon url input"),
            Self::Save => todo!("phase-E: save server icon url"),
        }
    }
}

/// Icon URL input, live preview, and save button.
///
/// Used by [`ServerOverviewSettings`] for both full and local-only modes.
#[ui_action(IconPanelAction)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
fn IconPanel(
    server_id: String,
    server_name: String,
    initial_url: String,
    local_only: bool,
) -> Element {
    let mut chat_data: Signal<ChatData> = use_context();
    let mut url_input = use_signal(|| initial_url);
    let mut saved = use_signal(|| false);
    let preview_url = url_input.read().clone();

    rsx! {
        div { class: "settings-section",
            h3 { class: "settings-section-title", "{t(\"server-overview-icon\")}" }

            // Live preview
            if !preview_url.is_empty() {
                div { class: "settings-preview-row",
                    img {
                        class: "settings-icon-preview",
                        src: "{preview_url}",
                        alt: "{server_name}",
                    }
                }
            }

            div { class: "settings-field",
                label { class: "settings-label", "{t(\"server-overview-icon-url\")}" }
                p { class: "settings-hint", "{t(\"server-overview-icon-hint\")}" }
                input {
                    r#type: "text",
                    class: "settings-input",
                    placeholder: "https://",
                    value: "{url_input}",
                    oninput: move |e| {
                        url_input.set(e.value());
                        saved.set(false);
                    },
                }
            }

            if local_only {
                p { class: "settings-hint settings-local-only-note",
                    "{t(\"server-overview-local-override-hint\")}"
                }
            }

            div { class: "settings-actions",
                button {
                    class: "btn-primary",
                    onclick: {
                        let sid = server_id.clone();
                        move |_| {
                            let url = url_input.read().clone();
                            // Update in-memory chat_data immediately
                            {
                                let mut cd = chat_data.write();
                                if let Some(s) = cd.servers.iter_mut().find(|s| s.id == sid) {
                                    s.icon_url = if url.is_empty() { None } else { Some(url.clone()) };
                                }
                                if let Some(ref mut cs) = cd.current_server
                                    && cs.id == sid
                                {
                                    cs.icon_url = if url.is_empty() { None } else { Some(url.clone()) };
                                }
                            }
                            // Persist override to storage
                            let sid2 = sid.clone();
                            spawn(async move {
                                if let Some(storage) = crate::STORAGE.get()
                                    && let Ok(mut settings) = storage.get_app_settings().await
                                {
                                    if url.is_empty() {
                                        settings.server_icon_overrides.remove(&sid2);
                                    } else {
                                        settings.server_icon_overrides.insert(sid2, url);
                                    }
                                    let _ = storage.set_app_settings(&settings).await;
                                }
                            });
                            saved.set(true);
                        }
                    },
                    "{t(\"server-overview-save\")}"
                }
                if saved() {
                    span { class: "settings-saved-badge", "✓ {t(\"server-overview-saved\")}" }
                }
            }
        }
    }
}

pub enum BannerPanelAction {
    SetUrl(String),
    Save,
}

impl UiAction for BannerPanelAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetUrl(_) => todo!("phase-E: update banner url input"),
            Self::Save => todo!("phase-E: save server banner url"),
        }
    }
}

/// Banner URL input, live preview, and save button.
///
/// Saves locally to `AppSettings` and also calls `ClientBackend::update_server_banner`
/// so backends that support it (poly-server, Discord, Lemmy) persist the change
/// remotely. Backends returning `NotSupported` are silently ignored.
#[ui_action(BannerPanelAction)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
fn BannerPanel(
    server_id: String,
    server_name: String,
    initial_url: String,
    account_id: String,
) -> Element {
    let mut chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let mut url_input = use_signal(|| initial_url);
    let mut saved = use_signal(|| false);
    let preview_url = url_input.read().clone();

    rsx! {
        div { class: "settings-section",
            h3 { class: "settings-section-title", "{t(\"server-overview-banner\")}" }

            // Live preview
            if !preview_url.is_empty() {
                div { class: "settings-preview-row",
                    img {
                        class: "settings-banner-preview",
                        src: "{preview_url}",
                        alt: "{server_name} banner",
                    }
                }
            }

            div { class: "settings-field",
                label { class: "settings-label", "{t(\"server-overview-banner-url\")}" }
                p { class: "settings-hint", "{t(\"server-overview-banner-hint\")}" }
                input {
                    r#type: "text",
                    class: "settings-input",
                    placeholder: "https://",
                    value: "{url_input}",
                    oninput: move |e| {
                        url_input.set(e.value());
                        saved.set(false);
                    },
                }
            }

            div { class: "settings-actions",
                button {
                    class: "btn-primary",
                    onclick: {
                        let sid = server_id.clone();
                        let aid = account_id.clone();
                        move |_| {
                            let url = url_input.read().clone();
                            // Update in-memory chat_data immediately
                            {
                                let mut cd = chat_data.write();
                                if let Some(s) = cd.servers.iter_mut().find(|s| s.id == sid) {
                                    s.banner_url = if url.is_empty() { None } else { Some(url.clone()) };
                                }
                                if let Some(ref mut cs) = cd.current_server
                                    && cs.id == sid
                                {
                                    cs.banner_url = if url.is_empty() {
                                        None
                                    } else {
                                        Some(url.clone())
                                    };
                                }
                            }
                            let sid2 = sid.clone();
                            let url2 = url.clone();
                            let aid2 = aid.clone();
                            let backend_arc = client_manager.read().get_backend(&aid2);
                            let toast_queue = try_consume_context::<Signal<Vec<ToastMessage>>>();
                            spawn(async move {
                                // 1. Persist local override
                                if let Some(storage) = crate::STORAGE.get()
                                    && let Ok(mut settings) = storage.get_app_settings().await
                                {
                                    if url2.is_empty() {
                                        settings.server_banner_overrides.remove(&sid2);
                                    } else {
                                        settings.server_banner_overrides.insert(sid2.clone(), url2.clone());
                                    }
                                    let _ = storage.set_app_settings(&settings).await;
                                }
                                // 2. Call backend API
                                if let Some(arc) = backend_arc {
                                    let banner_arg = if url2.is_empty() { None } else { Some(url2.as_str()) };
                                    let result = arc.read().await.update_server_banner(&sid2, banner_arg).await;
                                    match result {
                                        Ok(()) => {
                                            tracing::debug!("update_server_banner ok for {sid2}");
                                        }
                                        Err(poly_client::ClientError::NotSupported(_)) => {
                                            // Backend doesn't support remote banner updates — local-only is fine
                                        }
                                        Err(e) => {
                                            tracing::warn!("update_server_banner failed: {e:?}");
                                            if let Some(q) = toast_queue {
                                                push_toast(q, ToastMessage::new("server-overview-banner-save-failed", ToastTone::Error));
                                            }
                                        }
                                    }
                                }
                            });
                            saved.set(true);
                        }
                    },
                    "{t(\"server-overview-save\")}"
                }
                if saved() {
                    span { class: "settings-saved-badge", "✓ {t(\"server-overview-saved\")}" }
                }
            }
        }
    }
}

/// Overview settings panel — server icon and banner configuration.
///
/// The first section shown in server settings (default section).
/// Always renders icon and banner URL inputs with live previews. Backend-
/// specific gating (banner-only-for-some-backends, checkbox-gated icon for
/// Matrix/Teams) was removed in WP 3; plugin-declared `PerServer` settings
/// sections handle backend-specific overrides now.
///
/// `account_id` is forwarded to `BannerPanel` so it can call
/// `ClientBackend::update_server_banner` after persisting the local override.
#[ui_action(None)]
#[context_menu(none)]
#[component]
pub fn ServerOverviewSettings(
    server_id: String,
    server_name: String,
    backend_slug: String,
    account_id: String,
) -> Element {
    let _ = backend_slug; // backend slug no longer gates rendering; kept for prop stability
    let chat_data: Signal<ChatData> = use_context();

    // Read current icon / banner URLs from chat_data (may already include
    // any override applied by apply_server_icon_overrides).
    let current_icon = chat_data
        .read()
        .servers
        .iter()
        .find(|s| s.id == server_id)
        .and_then(|s| s.icon_url.clone())
        .unwrap_or_default();

    let current_banner = chat_data
        .read()
        .servers
        .iter()
        .find(|s| s.id == server_id)
        .and_then(|s| s.banner_url.clone())
        .unwrap_or_default();

    rsx! {
        // ── Icon ──────────────────────────────────────────────────────────
        IconPanel {
            server_id: server_id.clone(),
            server_name: server_name.clone(),
            initial_url: current_icon,
            local_only: false,
        }

        // ── Banner ────────────────────────────────────────────────────────
        BannerPanel {
            server_id: server_id.clone(),
            server_name: server_name.clone(),
            initial_url: current_banner,
            account_id: account_id.clone(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn icon_panel_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<IconPanelAction>();
        let _ = IconPanelAction::SetUrl("https://example.com/icon.png".to_string());
        let _ = IconPanelAction::Save;
    }

    #[test]
    fn banner_panel_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<BannerPanelAction>();
        let _ = BannerPanelAction::SetUrl("https://example.com/banner.png".to_string());
        let _ = BannerPanelAction::Save;
    }
}
