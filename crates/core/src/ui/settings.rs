//! Settings page — accounts, backup, identity, theme, language, appearance.

use crate::i18n::t;
use crate::state::{AppState, SettingsSection};
use crate::theme::{ThemeConfig, ThemePreset};
use dioxus::prelude::*;
use std::collections::HashMap;

// ── Custom select component ───────────────────────────────────────────────────

/// A (value, display-label) pair for [`PolySelect`].
#[derive(Clone, PartialEq)]
struct SelectOption {
    value: &'static str,
    label: &'static str,
}

/// Fully themed dropdown select — replaces the ugly native `<select>`.
///
/// The native OS select popup ignores CSS custom properties; this component
/// renders entirely in the webview so it respects the active theme.
#[component]
fn PolySelect(
    options: Vec<SelectOption>,
    /// Currently selected value.
    value: String,
    /// Called with the new value string when the user picks an option.
    onchange: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let current_label = options
        .iter()
        .find(|o| o.value == value)
        .map(|o| o.label)
        .unwrap_or(&value);

    rsx! {
        div { class: "poly-select",
            // Trigger button
            div {
                class: if *open.read() { "poly-select-trigger open" } else { "poly-select-trigger" },
                onclick: move |_| {
                    let v = *open.read();
                    open.set(!v);
                },
                span { class: "poly-select-current", "{current_label}" }
                span { class: "poly-select-chevron", "▾" }
            }
            // Options panel
            if *open.read() {
                div { class: "poly-select-menu",
                    for opt in &options {
                        {
                            let opt_value = opt.value;
                            let is_active = opt.value == value;
                            rsx! {
                                div {
                                    class: if is_active { "poly-select-option active" } else { "poly-select-option" },
                                    onclick: move |_| {
                                        open.set(false);
                                        onchange.call(opt_value.to_string());
                                    },
                                    "{opt.label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Settings page component.
///
/// Two-column layout: navigation sidebar + content area.
#[component]
pub fn SettingsPage(app_state: Signal<AppState>) -> Element {
    let section = app_state.read().settings_section;
    // Subscribe to locale signal so nav labels re-render on language change.
    let _locale = crate::i18n::use_locale().read().clone();

    rsx! {
        div { class: "settings-page",
            // Settings navigation
            nav { class: "settings-nav",
                SettingsNavItem {
                    label: t("settings-accounts"),
                    active: section == SettingsSection::Accounts,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Accounts,
                }
                SettingsNavItem {
                    label: t("settings-backup"),
                    active: section == SettingsSection::Backup,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Backup,
                }
                SettingsNavItem {
                    label: t("settings-identity"),
                    active: section == SettingsSection::Identity,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Identity,
                }
                SettingsNavItem {
                    label: t("settings-theme"),
                    active: section == SettingsSection::Theme,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Theme,
                }
                SettingsNavItem {
                    label: t("settings-language"),
                    active: section == SettingsSection::Language,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Language,
                }
                SettingsNavItem {
                    label: t("settings-appearance"),
                    active: section == SettingsSection::Appearance,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Appearance,
                }
                SettingsNavItem {
                    label: t("settings-general"),
                    active: section == SettingsSection::General,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::General,
                }
            }

            // Settings content
            div { class: "settings-content",
                match section {
                    SettingsSection::Accounts => rsx! {
                        AccountsSettings {}
                    },
                    SettingsSection::Backup => rsx! {
                        BackupSettings {}
                    },
                    SettingsSection::Identity => rsx! {
                        IdentitySettings {}
                    },
                    SettingsSection::Theme => rsx! {
                        ThemeSettings {}
                    },
                    SettingsSection::Language => rsx! {
                        LanguageSettings {}
                    },
                    SettingsSection::Appearance => rsx! {
                        AppearanceSettings {}
                    },
                    SettingsSection::General => rsx! {
                        GeneralSettings {}
                    },
                }
            }
        }
    }
}

/// Navigation item in the settings sidebar.
#[component]
fn SettingsNavItem(label: String, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div {
            class: if active { "settings-nav-item active" } else { "settings-nav-item" },
            onclick: move |evt| onclick.call(evt),
            "{label}"
        }
    }
}

/// Accounts settings section.
#[component]
fn AccountsSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-accounts\")}" }
            p { class: "settings-description", "{t(\"settings-accounts-description\")}" }
            // TODO(phase-2.7.9.2): Account list grouped by backend
            button { class: "btn btn-primary", "{t(\"settings-add-account\")}" }
        }
    }
}

/// Backup servers settings section.
///
/// Displays all stored backup servers with live status, toggle, and actions.
/// Provides an inline "add server" form that executes the full auth flow
/// (challenge → PoW → token) before persisting the server record.
// DECISION(DX-BACKUP-UI-1): Status is tracked in a HashMap<url, String> signal
// so each server row can display independent transient state without needing
// a separate component per row.
#[component]
fn BackupSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();

    // Loaded server records from storage.
    let mut servers: Signal<Vec<crate::storage::BackupServerRecord>> = use_signal(Vec::new);
    // Transient status text per server URL (not persisted).
    let statuses: Signal<HashMap<String, String>> = use_signal(HashMap::new);
    // Whether the "add server" form is expanded.
    let mut show_add_form = use_signal(|| false);
    // Add form fields.
    let mut add_url = use_signal(String::new);
    let mut add_label = use_signal(String::new);
    let mut add_passphrase = use_signal(String::new);
    // Status message shown inside the add form.
    let mut add_status = use_signal(String::new);
    // Whether the add form is currently connecting (disable button).
    let mut add_connecting = use_signal(|| false);
    // URL of server currently showing re-auth passphrase input.
    let mut reauth_url: Signal<Option<String>> = use_signal(|| None);
    let mut reauth_passphrase = use_signal(String::new);

    // Load servers from storage on mount.
    use_future(move || async move {
        if let Some(s) = crate::STORAGE.get() {
            match s.get_backup_servers().await {
                Ok(list) => servers.set(list),
                Err(e) => tracing::warn!("Failed to load backup servers: {e}"),
            }
        }
    });

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-backup\")}" }
            p { class: "settings-description", "{t(\"settings-backup-description\")}" }

            // ── Server list ───────────────────────────────────────────────────
            div { class: "backup-server-list",
                {
                    let srv_list = servers.read().clone();
                    if srv_list.is_empty() {
                        rsx! {
                            p { class: "backup-no-servers", "{t(\"settings-backup-no-servers\")}" }
                        }
                    } else {
                        rsx! {
                            for record in srv_list {
                                {
                                    let url = record.url.clone();
                                    let url_for_reauth = url.clone();
                                    let url_for_remove = url.clone();
                                    let url_for_sync = url.clone();
                                    let url_for_toggle = url.clone();
                                    let label = record.label.clone();
                                    let label_display = if label.is_empty() { url.clone() } else { label.clone() };
                                    let status_text = statuses
                                        .read()
                                        .get(&url)
                                        .cloned()
                                        .unwrap_or_else(|| t("settings-backup-status-unknown"));
                                    let token_present = record.token.is_some();
                                    let last_synced = record
                                        .last_synced_at
                                        .clone()
                                        .unwrap_or_else(|| t("settings-backup-never-synced"));
                                    let enabled = record.enabled;
                                    let is_reathing = reauth_url.read().as_deref() == Some(&url);

                                    let status_class = if status_text.contains("Connected") {
                                        "status-chip status-connected"
                                    } else if status_text.contains("Auth") || status_text.contains("required") {
                                        "status-chip status-auth-required"
                                    } else if status_text.contains("Sync") || status_text.contains("sync") {
                                        "status-chip status-syncing"
                                    } else if status_text.contains("failed") || status_text.contains("error") {
                                        "status-chip status-unreachable"
                                    } else {
                                        "status-chip status-unknown"
                                    };

                                    rsx! {
                                        div { class: "backup-server-row", key: "{url}",
                                            div { class: "backup-server-header",
                                                div { class: "backup-server-info",
                                                    span { class: "backup-server-label", "{label_display}" }
                                                    span { class: "backup-server-url", "{url}" }
                                                }
                                                div { class: "backup-server-meta",
                                                    span { class: "{status_class}", "{status_text}" }
                                                    span { class: "backup-last-synced", "{last_synced}" }
                                                }
                                            }
                                            div { class: "backup-server-actions",
                                                // Enabled toggle
                                                label { class: "toggle-label",
                                                    input {
                                                        r#type: "checkbox",
                                                        checked: enabled,
                                                        onchange: move |evt| {
                                                            let checked = evt.checked();
                                                            let url_t = url_for_toggle.clone();
                                                            spawn(async move {
                                                                if let Some(s) = crate::STORAGE.get() {
                                                                    match s.get_backup_servers().await {
                                                                        Ok(mut list) => {
                                                                            if let Some(srv) = list.iter_mut().find(|r| r.url == url_t) {
                                                                                srv.enabled = checked;
                                                                                let record_clone = srv.clone();
                                                                                if let Err(e) = s.upsert_backup_server(&record_clone).await {
                                                                                    tracing::error!("Failed to update server: {e}");
                                                                                } else {
                                                                                    servers.set(list);
                                                                                }
                                                                            }
                                                                        }
                                                                        Err(e) => tracing::error!("Failed to load servers: {e}"),
                                                                    }
                                                                }
                                                            });
                                                        },
                                                    }
                                                    " {t(\"settings-backup-enabled\")}"
                                                }

                                                // Sync Now button (only if we have a token)
                                                if token_present {
                                                    button {
                                                        class: "btn btn-sm btn-secondary",
                                                        onclick: move |_| {
                                                            let url_s = url_for_sync.clone();
                                                            let mut st = statuses;
                                                            spawn(async move {
                                                                st.write().insert(url_s.clone(), t("settings-backup-status-syncing"));
                                                                if let Some(storage) = crate::STORAGE.get() {
                                                                    match storage.get_backup_servers().await {
                                                                        Ok(list) => {
                                                                            if let Some(rec) = list.iter().find(|r| r.url == url_s) {
                                                                                let cfg = crate::sync::BackupServerConfig {
                                                                                    url: rec.url.clone(),
                                                                                    name: rec.label.clone(),
                                                                                    token: rec.token.clone(),
                                                                                    last_sequence: rec.last_sequence,
                                                                                };
                                                                                let client = crate::sync::SyncClient::new(cfg);
                                                                                match storage.get_app_settings().await {
                                                                                    Ok(settings) => {
                                                                                        let payload = serde_json::to_vec(&settings)
                                                                                            .unwrap_or_default();
                                                                                        if let Ok(identity_key) = storage.get_identity_key().await {
                                                                                            let enc_result = if let Some(key) = identity_key {
                                                                                                crate::crypto::encrypt(&payload, &key)
                                                                                            } else {
                                                                                                crate::crypto::encrypt(&payload, &[0u8; 32])
                                                                                            };
                                                                                            match enc_result {
                                                                                                Ok(encrypted) => {
                                                                                                    match client.push(&encrypted).await {
                                                                                                        Ok(seq) => {
                                                                                                            let now = chrono::Utc::now().to_rfc3339();
                                                                                                            match storage.get_backup_servers().await {
                                                                                                                Ok(mut servers_list) => {
                                                                                                                    if let Some(srv) = servers_list
                                                                                                                        .iter_mut()
                                                                                                                        .find(|r| r.url == url_s)
                                                                                                                    {
                                                                                                                        srv.last_sequence = seq;
                                                                                                                        srv.last_synced_at = Some(now.clone());
                                                                                                                        let updated = srv.clone();
                                                                                                                        if let Err(e) = storage.upsert_backup_server(&updated).await
                                                                                                                        {
                                                                                                                            tracing::error!("Failed to update server after sync: {e}");
                                                                                                                        } else {
                                                                                                                            servers.set(servers_list);
                                                                                                                            st.write()
                                                                                                                                .insert(url_s, t("settings-backup-status-connected"));
                                                                                                                        }
                                                                                                                    }
                                                                                                                }
                                                                                                                Err(e) => tracing::error!("Reload servers: {e}"),
                                                                                                            }
                                                                                                        }
                                                                                                        Err(e) => {
                                                                                                            let msg = if e.to_string().contains("token") {
                                                                                                                t("settings-backup-status-auth-required")
                                                                                                            } else {
                                                                                                                t("settings-backup-status-unreachable")
                                                                                                            };
                                                                                                            tracing::error!("Sync push error: {e}");
                                                                                                            st.write().insert(url_s, msg);
                                                                                                        }
                                                                                                    }
                                                                                                }
                                                                                                Err(e) => {
                                                                                                    tracing::error!("Encrypt failed: {e}");
                                                                                                    st.write()
                                                                                                        .insert(url_s, t("settings-backup-status-unreachable"));
                                                                                                }
                                                                                            }
                                                                                        } else {
                                                                                            st.write()
                                                                                                .insert(url_s, t("settings-backup-status-unreachable"));
                                                                                        }
                                                                                    }
                                                                                    Err(e) => {
                                                                                        tracing::error!("Load settings: {e}");
                                                                                        st.write()
                                                                                            .insert(url_s, t("settings-backup-status-unreachable"));
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                        Err(e) => tracing::error!("Load servers: {e}"),
                                                                    }
                                                                }
                                                            });
                                                        },
                                                        "{t(\"settings-backup-sync-now\")}"
                                                    }
                                                }

                                                // Re-authenticate button
                                                button {
                                                    class: "btn btn-sm btn-secondary",
                                                    onclick: move |_| {
                                                        reauth_url.set(Some(url_for_reauth.clone()));
                                                        reauth_passphrase.set(String::new());
                                                    },
                                                    "{t(\"settings-backup-reauth\")}"
                                                }

                                                // Remove button
                                                button {
                                                    class: "btn btn-sm btn-danger",
                                                    onclick: move |_| {
                                                        let url_r = url_for_remove.clone();
                                                        spawn(async move {
                                                            if let Some(s) = crate::STORAGE.get() {
                                                                if let Err(e) = s.remove_backup_server(&url_r).await {
                                                                    tracing::error!("Remove server: {e}");
                                                                }
                                                                match s.get_backup_servers().await {
                                                                    Ok(list) => servers.set(list),
                                                                    Err(e) => tracing::error!("Reload after remove: {e}"),
                                                                }
                                                            }
                                                        });
                                                    },
                                                    "{t(\"settings-backup-remove\")}"
                                                }
                                            }

                                            // Inline re-auth form
                                            if is_reathing {
                                                {
                                                    let url_ra = reauth_url.read().clone().unwrap_or_default();
                                                    rsx! {
                                                        div { class: "backup-reauth-form",
                                                            input {
                                                                r#type: "password",
                                                                class: "input-field",
                                                                placeholder: "{t(\"settings-backup-passphrase-label\")}",
                                                                value: "{reauth_passphrase}",
                                                                oninput: move |e| reauth_passphrase.set(e.value()),
                                                            }
                                                            div { class: "reauth-actions",
                                                                button {
                                                                    class: "btn btn-sm btn-primary",
                                                                    onclick: move |_| {
                                                                        let url_inner = url_ra.clone();
                                                                        let passphrase = reauth_passphrase.read().clone();
                                                                        let mut st = statuses;
                                                                        spawn(async move {
                                                                            st.write().insert(url_inner.clone(), t("settings-backup-connecting"));
                                                                            if let Some(storage) = crate::STORAGE.get() {
                                                                                match storage.get_app_settings().await {
                                                                                    Ok(settings) => {
                                                                                        let cfg = crate::sync::BackupServerConfig {
                                                                                            url: url_inner.clone(),
                                                                                            name: String::new(),
                                                                                            token: None,
                                                                                            last_sequence: 0,
                                                                                        };
                                                                                        let mut client = crate::sync::SyncClient::new(cfg);
                                                                                        match client
                                                                                            .authenticate(
                                                                                                &passphrase,
                                                                                                &settings.account_id,
                                                                                                "Poly Desktop",
                                                                                            )
                                                                                            .await
                                                                                        {
                                                                                            Ok(auth) => {
                                                                                                match storage.get_backup_servers().await {
                                                                                                    Ok(mut list) => {
                                                                                                        let opt_updated = list
                                                                                                            .iter_mut()
                                                                                                            .find(|r| r.url == url_inner)
                                                                                                            .map(|srv| {
                                                                                                                srv.token = Some(auth.token);
                                                                                                                srv.token_expires_at = auth.expires_at;
                                                                                                                srv.clone()
                                                                                                            });
                                                                                                        if let Some(updated) = opt_updated {
                                                                                                            match storage.upsert_backup_server(&updated).await {
                                                                                                                Ok(()) => {
                                                                                                                    servers.set(list);
                                                                                                                    st.write()
                                                                                                                        .insert(
                                                                                                                            url_inner.clone(),
                                                                                                                            t("settings-backup-status-connected"),
                                                                                                                        );
                                                                                                                    reauth_url.set(None);
                                                                                                                }
                                                                                                                Err(e) => tracing::error!("Save reauth token: {e}"),
                                                                                                            }
                                                                                                        }
                                                                                                    }
                                                                                                    Err(e) => tracing::error!("Reload: {e}"),
                                                                                                }
                                                                                            }
                                                                                            Err(e) => {
                                                                                                tracing::warn!("Re-auth failed: {e}");
                                                                                                st.write()
                                                                                                    .insert(url_inner, t("settings-backup-auth-failed"));
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                    Err(e) => tracing::error!("Load settings: {e}"),
                                                                                }
                                                                            }
                                                                        });
                                                                    },
                                                                    "{t(\"settings-backup-connect\")}"
                                                                }
                                                                button {
                                                                    class: "btn btn-sm btn-ghost",
                                                                    onclick: move |_| reauth_url.set(None),
                                                                    "{t(\"settings-backup-cancel\")}"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Add server button / form ───────────────────────────────────────
            if !*show_add_form.read() {
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        show_add_form.set(true);
                        add_url.set(String::new());
                        add_label.set(String::new());
                        add_passphrase.set(String::new());
                        add_status.set(String::new());
                    },
                    "{t(\"settings-backup-add-server\")}"
                }
            } else {
                div { class: "backup-add-form",
                    div { class: "form-group",
                        label { "{t(\"settings-backup-url-label\")}" }
                        input {
                            r#type: "text",
                            class: "input-field",
                            placeholder: "{t(\"settings-backup-url-placeholder\")}",
                            value: "{add_url}",
                            oninput: move |e| add_url.set(e.value()),
                        }
                    }
                    div { class: "form-group",
                        label { "{t(\"settings-backup-label-label\")}" }
                        input {
                            r#type: "text",
                            class: "input-field",
                            placeholder: "My Backup Server",
                            value: "{add_label}",
                            oninput: move |e| add_label.set(e.value()),
                        }
                    }
                    div { class: "form-group",
                        label { "{t(\"settings-backup-passphrase-label\")}" }
                        input {
                            r#type: "password",
                            class: "input-field",
                            value: "{add_passphrase}",
                            oninput: move |e| add_passphrase.set(e.value()),
                        }
                    }
                    if !add_status.read().is_empty() {
                        p { class: "form-status", "{add_status}" }
                    }
                    div { class: "form-actions",
                        button {
                            class: "btn btn-primary",
                            disabled: *add_connecting.read(),
                            onclick: move |_| {
                                let url = add_url.read().trim().to_string();
                                let label = add_label.read().trim().to_string();
                                let passphrase = add_passphrase.read().clone();
                                if url.is_empty() {
                                    add_status.set("Please enter a server URL.".to_string());
                                    return;
                                }
                                add_connecting.set(true);
                                add_status.set(t("settings-backup-connecting"));

                                spawn(async move {
                                    if let Some(storage) = crate::STORAGE.get() {
                                        match storage.get_app_settings().await {
                                            Ok(settings) => {
                                                let cfg = crate::sync::BackupServerConfig {
                                                    url: url.clone(),
                                                    name: label.clone(),
                                                    token: None,
                                                    last_sequence: 0,
                                                };
                                                let mut client = crate::sync::SyncClient::new(cfg);
                                                match client
                                                    .authenticate(
                                                        &passphrase,
                                                        &settings.account_id,
                                                        "Poly Desktop",
                                                    )
                                                    .await
                                                {
                                                    Ok(auth) => {
                                                        let record = crate::storage::BackupServerRecord {
                                                            url: url.clone(),
                                                            label: if label.is_empty() { url.clone() } else { label },
                                                            enabled: true,
                                                            last_sequence: 0,
                                                            token: Some(auth.token),
                                                            token_expires_at: auth.expires_at,
                                                            last_synced_at: None,
                                                        };
                                                        match storage.upsert_backup_server(&record).await {
                                                            Ok(()) => {
                                                                match storage.get_backup_servers().await {
                                                                    Ok(list) => {
                                                                        servers.set(list);
                                                                        add_status.set(t("settings-backup-auth-success"));
                                                                        add_connecting.set(false);
                                                                        show_add_form.set(false);
                                                                    }
                                                                    Err(e) => {
                                                                        add_status.set(format!("Reload error: {e}"));
                                                                        add_connecting.set(false);
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                add_status.set(format!("Save error: {e}"));
                                                                add_connecting.set(false);
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        tracing::warn!("Add server auth failed: {e}");
                                                        add_status.set(t("settings-backup-auth-failed"));
                                                        add_connecting.set(false);
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                add_status.set(format!("Load settings error: {e}"));
                                                add_connecting.set(false);
                                            }
                                        }
                                    }
                                });
                            },
                            if *add_connecting.read() {
                                "{t(\"settings-backup-connecting\")}"
                            } else {
                                "{t(\"settings-backup-connect\")}"
                            }
                        }
                        button {
                            class: "btn btn-ghost",
                            onclick: move |_| show_add_form.set(false),
                            "{t(\"settings-backup-cancel\")}"
                        }
                    }
                }
            }
        }
    }
}

/// Identity settings section.
///
/// Displays the user's Ed25519 public key (Account ID) and provides a
/// "Show Recovery Phrase" button that reveals the 24-word BIP39 mnemonic
/// in an overlay modal.
#[component]
fn IdentitySettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();

    // The hex-encoded public key loaded from storage.
    let mut account_id = use_signal(String::new);
    // Whether the mnemonic modal is visible.
    let mut show_phrase_modal = use_signal(|| false);
    // The 24 mnemonic words (populated when modal opens).
    let mut mnemonic_words: Signal<Vec<String>> = use_signal(Vec::new);
    // Status message for the identity section (e.g. errors).
    let mut status_msg = use_signal(String::new);

    // Load account ID from storage on mount.
    use_future(move || async move {
        if let Some(s) = crate::STORAGE.get() {
            match s.get_app_settings().await {
                Ok(settings) if !settings.account_id.is_empty() => {
                    account_id.set(settings.account_id);
                }
                Ok(_) => {
                    status_msg.set(t("settings-identity-no-identity"));
                }
                Err(e) => {
                    tracing::warn!("Failed to load identity: {e}");
                    status_msg.set(t("settings-identity-no-identity"));
                }
            }
        }
    });

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-identity\")}" }
            p { class: "settings-description", "{t(\"settings-identity-description\")}" }

            if !account_id.read().is_empty() {
                div { class: "identity-info",
                    label { class: "settings-label", "{t(\"settings-identity-your-id-label\")}" }
                    div { class: "account-id-row",
                        code { class: "account-id", "{account_id}" }
                        button {
                            class: "btn btn-sm btn-ghost",
                            onclick: move |_| {
                                // Copy to clipboard via eval
                                let id = account_id.read().clone();
                                let js = format!("navigator.clipboard.writeText({:?}).catch(() => {{}})", id);
                                let _ = document::eval(&js);
                            },
                            "{t(\"settings-identity-copy-id\")}"
                        }
                    }
                }

                button {
                    class: "btn btn-secondary",
                    onclick: move |_| {
                        // Load private key and generate mnemonic
                        spawn(async move {
                            if let Some(s) = crate::STORAGE.get() {
                                match s.get_identity_key().await {
                                    Ok(Some(key_bytes)) => {
                                        let identity = crate::crypto::Identity::from_private_key_bytes(
                                            &key_bytes,
                                        );
                                        match identity.to_mnemonic() {
                                            Ok(phrase) => {
                                                let words: Vec<String> = phrase
                                                    .split_whitespace()
                                                    .map(str::to_string)
                                                    .collect();
                                                mnemonic_words.set(words);
                                                show_phrase_modal.set(true);
                                            }
                                            Err(e) => {
                                                tracing::error!("Mnemonic generation failed: {e}");
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        tracing::warn!("No identity key in storage");
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to load identity key: {e}");
                                    }
                                }
                            }
                        });
                    },
                    "{t(\"settings-identity-show-phrase\")}"
                }
            } else {
                p { class: "settings-info", "{status_msg}" }
            }

            // ── Mnemonic modal ────────────────────────────────────────────────
            if *show_phrase_modal.read() {
                div {
                    class: "modal-overlay",
                    onclick: move |_| show_phrase_modal.set(false),
                    div {
                        class: "modal-content",
                        // Stop clicks on the modal from bubbling to the overlay
                        onclick: move |e| e.stop_propagation(),

                        h3 { class: "modal-title", "{t(\"settings-identity-phrase-modal-title\")}" }
                        p { class: "modal-warning", "{t(\"settings-identity-phrase-warning\")}" }

                        div { class: "mnemonic-grid",
                            {
                                let words = mnemonic_words.read().clone();
                                words
                                    .into_iter()
                                    .enumerate()
                                    .map(|(i, word)| {
                                        rsx! {
                                            div { class: "mnemonic-word", key: "{i}",
                                                span { class: "word-number", "{i + 1}." }
                                                span { class: "word-text", "{word}" }
                                            }
                                        }
                                    })
                            }
                        }

                        div { class: "modal-actions",
                            button {
                                class: "btn btn-secondary",
                                onclick: move |_| {
                                    let phrase = mnemonic_words.read().join(" ");
                                    let js = format!(
                                        "navigator.clipboard.writeText({:?}).catch(() => {{}})",
                                        phrase,
                                    );
                                    let _ = document::eval(&js);
                                },
                                "{t(\"settings-identity-copy-all\")}"
                            }
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| show_phrase_modal.set(false),
                                "{t(\"settings-identity-close\")}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Theme settings section.
///
/// Reads/writes the `Signal<ThemeConfig>` provided by [`crate::ui::App`].
/// Changing the preset updates the signal immediately (re-renders the
/// `<style id="poly-theme">` in App) and persists to storage.
#[component]
fn ThemeSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let mut theme_config = use_context::<Signal<ThemeConfig>>();

    let current_preset = match theme_config.read().preset {
        ThemePreset::NeutralDark => "neutral-dark",
        ThemePreset::Purple => "purple",
        ThemePreset::Red => "red",
        ThemePreset::Custom => "custom",
    };

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-theme\")}" }
            p { class: "settings-description", "{t(\"settings-theme-description\")}" }
            div { class: "theme-presets",
                label { class: "settings-label", "{t(\"settings-theme-preset\")}" }
                PolySelect {
                    options: vec![
                        SelectOption {
                            value: "neutral-dark",
                            label: "Neutral Dark",
                        },
                        SelectOption {
                            value: "purple",
                            label: "Purple",
                        },
                        SelectOption {
                            value: "red",
                            label: "Red",
                        },
                        SelectOption {
                            value: "custom",
                            label: "Custom",
                        },
                    ],
                    value: current_preset.to_string(),
                    onchange: move |new_val: String| {
                        let preset = match new_val.as_str() {
                            "purple" => ThemePreset::Purple,
                            "red" => ThemePreset::Red,
                            "custom" => ThemePreset::Custom,
                            _ => ThemePreset::NeutralDark,
                        };
                        let mut new_config = theme_config.read().clone();
                        new_config.preset = preset;
                        theme_config.set(new_config.clone());
                        spawn(async move {
                            if let Some(s) = crate::STORAGE.get() {
                                if let Err(e) = s.set_theme_config(&new_config).await {
                                    tracing::error!("Failed to persist theme config: {e}");
                                } else {
                                    tracing::info!("Theme config persisted ✓");
                                }
                            }
                        });
                    },
                }
            }
            div { class: "theme-actions",
                button { class: "btn btn-secondary", "{t(\"settings-theme-import\")}" }
                button { class: "btn btn-secondary", "{t(\"settings-theme-export\")}" }
            }
        }
    }
}

/// Language settings section.
///
/// The dropdown pre-selects the OS/browser-detected language (set during
/// [`crate::i18n::init`]) and switches the entire app's strings reactively
/// on change. Works identically on desktop (Wry) and web (WASM).
#[component]
fn LanguageSettings() -> Element {
    // Reads the locale Signal from context — subscribes to changes so the
    // selected option updates immediately when another part of the app
    // changes the locale.
    let mut locale_sig = crate::i18n::use_locale();
    let current_locale = locale_sig.read().clone();

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-language\")}" }
            p { class: "settings-description", "{t(\"settings-language-description\")}" }
            PolySelect {
                options: vec![
                    SelectOption {
                        value: "en",
                        label: "English",
                    },
                    SelectOption {
                        value: "de",
                        label: "Deutsch",
                    },
                    SelectOption {
                        value: "fr",
                        label: "Français",
                    },
                    SelectOption {
                        value: "es",
                        label: "Español",
                    },
                ],
                value: current_locale.clone(),
                onchange: move |new_locale: String| {
                    // Update global i18n state and re-render all subscribed
                    // components via the shared Signal.
                    crate::i18n::set_locale(&new_locale);
                    *locale_sig.write() = new_locale.clone();
                    // Persist (fire-and-forget).
                    spawn(async move {
                        if let Some(s) = crate::STORAGE.get() {
                            match s.get_app_settings().await {
                                Ok(mut settings) => {
                                    settings.locale = new_locale;
                                    if let Err(e) = s.set_app_settings(&settings).await {
                                        tracing::error!("Failed to persist locale: {e}");
                                    } else {
                                        tracing::info!("Locale persisted to storage ✓");
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to read settings for locale persist: {e}"
                                    )
                                }
                            }
                        }
                    });
                },
            }
        }
    }
}

/// Appearance settings section.
#[component]
fn AppearanceSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-appearance\")}" }
            p { class: "settings-description", "{t(\"settings-appearance-description\")}" }
            // TODO(phase-2.7.9.9): Dark/light mode toggle
            div { class: "appearance-options",
                label {
                    input {
                        r#type: "radio",
                        name: "color-mode",
                        value: "dark",
                        checked: true,
                    }
                    " {t(\"settings-dark-mode\")}"
                }
                label {
                    input { r#type: "radio", name: "color-mode", value: "light" }
                    " {t(\"settings-light-mode\")}"
                }
                label {
                    input {
                        r#type: "radio",
                        name: "color-mode",
                        value: "follow",
                    }
                    " {t(\"settings-follow-device\")}"
                }
            }
        }
    }
}

/// General settings section.
#[component]
fn GeneralSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-general\")}" }
            p { class: "settings-description", "{t(\"settings-general-description\")}" }
                // TODO(phase-2.7.9.10): Notification preferences, startup behavior
        }
    }
}
