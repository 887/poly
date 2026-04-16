//! Server overview settings — icon and banner configuration.
//!
//! Allows users to customise the server icon and banner image URLs.
//!
//! ## Backend-awareness
//! | Backend | Icon | Banner | Notes |
//! |---|---|---|---|
//! | Demo, Stoat, Discord, Poly | ✅ | ✅ | Stored locally; Phase 3 adds API calls |
//! | Matrix, Teams | ✅ (opt-in) | — | Checkbox enables local-only override |
//!
//! ## Components
//! - [`ServerOverviewSettings`] — root, dispatches to sub-panels
//! - [`IconPanel`] — icon URL input + preview
//! - [`BannerPanel`] — banner URL input + preview

use crate::i18n::t;
use crate::state::ChatData;
use dioxus::prelude::*;
use poly_client::BackendType;

/// Determine whether a backend slug identifies a backend that owns its servers
/// and can set icon/banner programmatically (Phase 3 API calls).
/// For Phase 2, all writes are local-only even for supported backends.
fn backend_from_slug(slug: &str) -> Option<BackendType> {
    match slug {
        "" => None,
        s => Some(BackendType::from(s)),
    }
}

/// Returns `true` for backends that support user-facing banner images.
fn supports_banner(backend: Option<&BackendType>) -> bool {
    backend.is_some_and(|b| matches!(b.as_str(), "demo" | "stoat" | "discord" | "poly"))
}

/// Returns `true` for backends where server icon changes must be local-only
/// (Matrix workspaces, Teams channels — no "server" ownership).
fn is_local_only(backend: Option<&BackendType>) -> bool {
    backend.is_some_and(|b| matches!(b.as_str(), "matrix" | "teams"))
}

/// Icon URL input, live preview, and save button.
///
/// Used by [`ServerOverviewSettings`] for both full and local-only modes.
#[rustfmt::skip]
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

/// Banner URL input, live preview, and save button.
///
/// Shown only for backends that support banner images (Demo, Stoat, Discord, Poly).
#[rustfmt::skip]
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
/// Shows icon and banner URL inputs with live previews. For Matrix/Teams
/// backends, the icon section is gated behind a "local override" checkbox
/// since those backends don't have user-owned servers.
///
/// # Phase 3 note
/// <!-- TODO(phase-3): wire icon/banner saves to backend API calls -->
/// Currently all saves are local-only (stored in `AppSettings`). Phase 3 will
/// add `ClientBackend::update_server_icon` / `update_server_banner` for
/// Demo, Stoat, Discord, and Poly backends.
#[component]
pub fn ServerOverviewSettings(
    server_id: String,
    server_name: String,
    backend_slug: String,
) -> Element {
    let chat_data: Signal<ChatData> = use_context();

    let backend = backend_from_slug(&backend_slug);
    let show_banner = supports_banner(backend.as_ref());
    let local_only = is_local_only(backend.as_ref());

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

    // For local-only backends, gate the icon panel behind a checkbox.
    let mut icon_override_enabled = use_signal(|| !current_icon.is_empty());

    rsx! {
        // ── Icon ──────────────────────────────────────────────────────────
        if local_only {
            div { class: "settings-section",
                h3 { class: "settings-section-title", "{t(\"server-overview-icon\")}" }
                label { class: "settings-checkbox-label",
                    input {
                        r#type: "checkbox",
                        checked: icon_override_enabled(),
                        onchange: move |e| icon_override_enabled.set(e.checked()),
                    }
                    span { " {t(\"server-overview-local-override\")}" }
                }
            }
            if icon_override_enabled() {
                IconPanel {
                    server_id: server_id.clone(),
                    server_name: server_name.clone(),
                    initial_url: current_icon,
                    local_only: true,
                }
            }
        } else {
            IconPanel {
                server_id: server_id.clone(),
                server_name: server_name.clone(),
                initial_url: current_icon,
                local_only: false,
            }
        }

        // ── Banner ────────────────────────────────────────────────────────
        if show_banner {
            BannerPanel {
                server_id: server_id.clone(),
                server_name: server_name.clone(),
                initial_url: current_banner,
            }
        }
    }
}
