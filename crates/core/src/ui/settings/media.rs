//! Media settings — external GIF providers and future rich-media integrations.
//!
//! Keeps provider config app-scoped (not backend-scoped) because GIF search is
//! an external integration, unlike emojis/stickers which are loaded per client.

use super::common::{PolySelect, SelectOption};
use crate::i18n::t;
use crate::storage::{GifProviderKind, MediaProviderSettings};
use dioxus::prelude::*;

async fn persist_media_settings(media: MediaProviderSettings) {
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(mut app_settings) = storage.get_app_settings().await else {
        return;
    };
    app_settings.media = media;
    if let Err(err) = storage.set_app_settings(&app_settings).await {
        tracing::warn!("Failed to persist media settings: {err}");
    }
}

fn provider_options() -> Vec<SelectOption> {
    vec![
        SelectOption {
            value: GifProviderKind::Klippy.as_str(),
            label: "Klippy",
        },
        SelectOption {
            value: GifProviderKind::Giphy.as_str(),
            label: "Giphy",
        },
        SelectOption {
            value: GifProviderKind::Imgur.as_str(),
            label: "Imgur",
        },
    ]
}

fn update_media_settings(
    mut media: Signal<MediaProviderSettings>,
    update: impl FnOnce(&mut MediaProviderSettings),
) {
    let mut next = media.read().clone();
    update(&mut next);
    media.set(next.clone());
    spawn(async move {
        persist_media_settings(next).await;
    });
}

fn load_media_settings(mut media: Signal<MediaProviderSettings>) {
    spawn(async move {
        let Some(storage) = crate::STORAGE.get() else {
            return;
        };
        match storage.get_app_settings().await {
            Ok(app_settings) => media.set(app_settings.media),
            Err(err) => tracing::warn!("Failed to load media settings: {err}"),
        }
    });
}

#[rustfmt::skip]
#[component]
fn ProviderCard(
    title: String,
    api_key: String,
    enabled: bool,
    configured: bool,
    on_toggle: EventHandler<bool>,
    on_key_input: EventHandler<String>,
) -> Element {
    let status_key = if configured {
        "settings-media-status-configured"
    } else {
        "settings-media-status-not-setup"
    };
    let api_key_label = t("settings-media-api-key");
    let api_key_placeholder = t("settings-media-api-key-placeholder");

    rsx! {
        div { class: "media-provider-card",
            div { class: "media-provider-header",
                div {
                    h3 { class: "media-provider-title", "{title}" }
                    p { class: "media-provider-status", {t(status_key)} }
                }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: enabled,
                        onchange: move |evt| on_toggle.call(evt.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
            label { class: "settings-label", "{api_key_label}" }
            input {
                r#type: "password",
                class: "settings-text-input",
                value: "{api_key}",
                placeholder: "{api_key_placeholder}",
                oninput: move |evt| on_key_input.call(evt.value()),
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn ActiveProviderSelector(active_value: String, on_change: EventHandler<String>) -> Element {
    rsx! {
        div { class: "media-settings-active-provider",
            label { class: "settings-label", "{t(\"settings-media-active-provider\")}" }
            PolySelect {
                options: provider_options(),
                value: active_value,
                onchange: move |value: String| on_change.call(value),
            }
        }
    }
}

#[component]
fn ProviderCards(
    klippy_api_key: String,
    klippy_enabled: bool,
    giphy_api_key: String,
    giphy_enabled: bool,
    imgur_api_key: String,
    imgur_enabled: bool,
    media: Signal<MediaProviderSettings>,
) -> Element {
    rsx! {
        ProviderCard {
            title: t("settings-media-provider-klippy"),
            api_key: klippy_api_key.clone(),
            enabled: klippy_enabled,
            configured: !klippy_api_key.trim().is_empty(),
            on_toggle: move |enabled| {
                update_media_settings(media, |next| next.klippy.enabled = enabled);
            },
            on_key_input: move |value: String| {
                update_media_settings(media, |next| next.klippy.api_key = value);
            },
        }

        ProviderCard {
            title: t("settings-media-provider-giphy"),
            api_key: giphy_api_key.clone(),
            enabled: giphy_enabled,
            configured: !giphy_api_key.trim().is_empty(),
            on_toggle: move |enabled| {
                update_media_settings(media, |next| next.giphy.enabled = enabled);
            },
            on_key_input: move |value: String| {
                update_media_settings(media, |next| next.giphy.api_key = value);
            },
        }

        ProviderCard {
            title: t("settings-media-provider-imgur"),
            api_key: imgur_api_key.clone(),
            enabled: imgur_enabled,
            configured: !imgur_api_key.trim().is_empty(),
            on_toggle: move |enabled| {
                update_media_settings(media, |next| next.imgur.enabled = enabled);
            },
            on_key_input: move |value: String| {
                update_media_settings(media, |next| next.imgur.api_key = value);
            },
        }
    }
}

/// Media integrations settings section.
#[component]
pub(super) fn MediaSettings() -> Element {
    let media = use_signal(MediaProviderSettings::default);
    let mut loaded = use_signal(|| false);

    use_effect(move || {
        if *loaded.read() {
            return;
        }
        loaded.set(true);
        load_media_settings(media);
    });

    let current = media.read().clone();

    rsx! {
        div { class: "settings-section media-settings",
            h2 { "{t(\"settings-media\")}" }
            p { class: "settings-description", "{t(\"settings-media-description\")}" }

            ActiveProviderSelector {
                active_value: current.active_gif_provider.as_str().to_string(),
                on_change: move |value: String| {
                    if let Some(kind) = GifProviderKind::from_slug(&value) {
                        update_media_settings(media, |next| next.active_gif_provider = kind);
                    }
                },
            }
            ProviderCards {
                klippy_api_key: current.klippy.api_key.clone(),
                klippy_enabled: current.klippy.enabled,
                giphy_api_key: current.giphy.api_key.clone(),
                giphy_enabled: current.giphy.enabled,
                imgur_api_key: current.imgur.api_key.clone(),
                imgur_enabled: current.imgur.enabled,
                media,
            }
        }
    }
}
