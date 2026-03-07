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

/// Media integrations settings section.
#[component]
pub(super) fn MediaSettings() -> Element {
    let mut media = use_signal(MediaProviderSettings::default);
    let mut loaded = use_signal(|| false);

    use_effect(move || {
        if *loaded.read() {
            return;
        }
        loaded.set(true);
        spawn(async move {
            let Some(storage) = crate::STORAGE.get() else {
                return;
            };
            match storage.get_app_settings().await {
                Ok(app_settings) => media.set(app_settings.media),
                Err(err) => tracing::warn!("Failed to load media settings: {err}"),
            }
        });
    });

    let current = media.read().clone();
    let active_value = current.active_gif_provider.as_str().to_string();
    let title = t("settings-media");
    let description = t("settings-media-description");
    let active_label = t("settings-media-active-provider");

    rsx! {
        div { class: "settings-section media-settings",
            h2 { "{title}" }
            p { class: "settings-description", "{description}" }

            div { class: "media-settings-active-provider",
                label { class: "settings-label", "{active_label}" }
                PolySelect {
                    options: provider_options(),
                    value: active_value,
                    onchange: move |value: String| {
                        if let Some(kind) = GifProviderKind::from_slug(&value) {
                            let mut next = media.read().clone();
                            next.active_gif_provider = kind;
                            media.set(next.clone());
                            spawn(async move {
                                persist_media_settings(next).await;
                            });
                        }
                    },
                }
            }

            ProviderCard {
                title: t("settings-media-provider-klippy"),
                api_key: current.klippy.api_key.clone(),
                enabled: current.klippy.enabled,
                configured: !current.klippy.api_key.trim().is_empty(),
                on_toggle: move |enabled| {
                    let mut next = media.read().clone();
                    next.klippy.enabled = enabled;
                    media.set(next.clone());
                    spawn(async move {
                        persist_media_settings(next).await;
                    });
                },
                on_key_input: move |value: String| {
                    let mut next = media.read().clone();
                    next.klippy.api_key = value;
                    media.set(next.clone());
                    spawn(async move {
                        persist_media_settings(next).await;
                    });
                },
            }

            ProviderCard {
                title: t("settings-media-provider-giphy"),
                api_key: current.giphy.api_key.clone(),
                enabled: current.giphy.enabled,
                configured: !current.giphy.api_key.trim().is_empty(),
                on_toggle: move |enabled| {
                    let mut next = media.read().clone();
                    next.giphy.enabled = enabled;
                    media.set(next.clone());
                    spawn(async move {
                        persist_media_settings(next).await;
                    });
                },
                on_key_input: move |value: String| {
                    let mut next = media.read().clone();
                    next.giphy.api_key = value;
                    media.set(next.clone());
                    spawn(async move {
                        persist_media_settings(next).await;
                    });
                },
            }

            ProviderCard {
                title: t("settings-media-provider-imgur"),
                api_key: current.imgur.api_key.clone(),
                enabled: current.imgur.enabled,
                configured: !current.imgur.api_key.trim().is_empty(),
                on_toggle: move |enabled| {
                    let mut next = media.read().clone();
                    next.imgur.enabled = enabled;
                    media.set(next.clone());
                    spawn(async move {
                        persist_media_settings(next).await;
                    });
                },
                on_key_input: move |value: String| {
                    let mut next = media.read().clone();
                    next.imgur.api_key = value;
                    media.set(next.clone());
                    spawn(async move {
                        persist_media_settings(next).await;
                    });
                },
            }
        }
    }
}
