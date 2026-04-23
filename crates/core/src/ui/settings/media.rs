//! Media settings — external GIF providers and future rich-media integrations.
//!
//! Each provider has a tab. The tabs show all providers and their config.
//! Only enabled providers appear as selectable tabs in the GIF picker in chat;
//! the chat picker tab for each provider maps to the `active_gif_provider`.

use crate::i18n::t;
use crate::storage::{GifProviderKind, MediaProviderSettings};
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the media settings section.
pub enum MediaSettingsAction {
    /// Toggle a GIF provider on or off.
    ToggleProvider(GifProviderKind, bool),
    /// Update the API key for a GIF provider.
    SetProviderApiKey(GifProviderKind, String),
    /// Switch the active (default) GIF provider.
    SetActiveProvider(GifProviderKind),
}

impl UiAction for MediaSettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::ToggleProvider(_kind, _enabled) => todo!("phase-E: toggle GIF provider"),
            Self::SetProviderApiKey(_kind, _key) => todo!("phase-E: persist provider API key"),
            Self::SetActiveProvider(_kind) => todo!("phase-E: set active GIF provider"),
        }
    }
}

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

/// Single provider config panel (shown when that provider's tab is active).
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ProviderPanel(
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

/// Tab bar for provider selection — each provider is a clickable tab.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ProviderTabs(active_tab: Signal<GifProviderKind>, providers: Vec<(GifProviderKind, String)>) -> Element {
    rsx! {
        div { class: "media-provider-tabs",
            for (kind, label) in providers {
                {
                    let is_active = *active_tab.read() == kind;
                    rsx! {
                        button {
                            key: "{kind.as_str()}",
                            class: if is_active { "media-provider-tab active" } else { "media-provider-tab" },
                            onclick: move |_| *active_tab.write() = kind,
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

/// Media integrations settings section — provider tabs layout.
#[rustfmt::skip]
#[ui_action(MediaSettingsAction)]
#[context_menu(none)]
#[component]
pub(super) fn MediaSettings() -> Element {
    let media = use_signal(MediaProviderSettings::default);
    let mut loaded = use_signal(|| false);
    let mut active_tab: Signal<GifProviderKind> = use_signal(|| GifProviderKind::Klippy);

    use_effect(move || {
        if *loaded.read() {
            return;
        }
        loaded.set(true);
        load_media_settings(media);
    });

    let current = media.read().clone();

    // Sync active_tab to active_gif_provider on first load
    use_effect(move || {
        let provider = media.read().active_gif_provider;
        *active_tab.write() = provider;
    });

    let tab_providers = vec![
        (GifProviderKind::Klippy, t("settings-media-provider-klippy")),
        (GifProviderKind::Giphy, t("settings-media-provider-giphy")),
        (GifProviderKind::Imgur, t("settings-media-provider-imgur")),
    ];

    let tab = *active_tab.read();

    rsx! {
        div { class: "settings-section media-settings",
            h2 { "{t(\"settings-media\")}" }
            p { class: "settings-description", "{t(\"settings-media-description-tabs\")}" }

            ProviderTabs {
                active_tab,
                providers: tab_providers,
            }

            div { class: "media-provider-tab-content",
                match tab {
                    GifProviderKind::Klippy => rsx! {
                        ProviderPanel {
                            title: t("settings-media-provider-klippy"),
                            api_key: current.klippy.api_key.clone(),
                            enabled: current.klippy.enabled,
                            configured: !current.klippy.api_key.trim().is_empty(),
                            on_toggle: move |enabled| {
                                update_media_settings(media, |next| {
                                    next.klippy.enabled = enabled;
                                    if enabled { next.active_gif_provider = GifProviderKind::Klippy; }
                                });
                            },
                            on_key_input: move |value: String| {
                                update_media_settings(media, |next| next.klippy.api_key = value);
                            },
                        }
                    },
                    GifProviderKind::Giphy => rsx! {
                        ProviderPanel {
                            title: t("settings-media-provider-giphy"),
                            api_key: current.giphy.api_key.clone(),
                            enabled: current.giphy.enabled,
                            configured: !current.giphy.api_key.trim().is_empty(),
                            on_toggle: move |enabled| {
                                update_media_settings(media, |next| {
                                    next.giphy.enabled = enabled;
                                    if enabled { next.active_gif_provider = GifProviderKind::Giphy; }
                                });
                            },
                            on_key_input: move |value: String| {
                                update_media_settings(media, |next| next.giphy.api_key = value);
                            },
                        }
                    },
                    GifProviderKind::Imgur => rsx! {
                        ProviderPanel {
                            title: t("settings-media-provider-imgur"),
                            api_key: current.imgur.api_key.clone(),
                            enabled: current.imgur.enabled,
                            configured: !current.imgur.api_key.trim().is_empty(),
                            on_toggle: move |enabled| {
                                update_media_settings(media, |next| {
                                    next.imgur.enabled = enabled;
                                    if enabled { next.active_gif_provider = GifProviderKind::Imgur; }
                                });
                            },
                            on_key_input: move |value: String| {
                                update_media_settings(media, |next| next.imgur.api_key = value);
                            },
                        }
                    },
                }
            }

            // Note: Enabling a provider also sets it as the active chat GIF tab.
            p { class: "settings-description media-active-hint",
                "{t(\"settings-media-active-hint\")}"
            }
        }
    }
}
