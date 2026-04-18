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

use crate::i18n::t;
use crate::state::ChatData;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
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
#[context_menu(inherit)]
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
/// Shown only for backends that support banner images (Demo, Stoat, Discord, Poly).
#[ui_action(BannerPanelAction)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn BannerPanel(server_id: String, server_name: String, initial_url: String) -> Element {
    let mut chat_data: Signal<ChatData> = use_context();
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
                        move |_| {
                            let url = url_input.read().clone();
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
                            spawn(async move {
                                if let Some(storage) = crate::STORAGE.get()
                                    && let Ok(mut settings) = storage.get_app_settings().await
                                {
                                    if url.is_empty() {
                                        settings.server_banner_overrides.remove(&sid2);
                                    } else {
                                        settings.server_banner_overrides.insert(sid2, url);
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

/// Overview settings panel — server icon and banner configuration.
///
/// The first section shown in server settings (default section).
/// Always renders icon and banner URL inputs with live previews. Backend-
/// specific gating (banner-only-for-some-backends, checkbox-gated icon for
/// Matrix/Teams) was removed in WP 3; plugin-declared `PerServer` settings
/// sections handle backend-specific overrides now.
///
/// # Phase 3 note
/// <!-- TODO(phase-3): wire icon/banner saves to backend API calls -->
/// Currently all saves are local-only (stored in `AppSettings`). Phase 3 will
/// add `ClientBackend::update_server_icon` / `update_server_banner` for
/// backends that support programmatic server-asset writes.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ServerOverviewSettings(
    server_id: String,
    server_name: String,
    backend_slug: String,
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
