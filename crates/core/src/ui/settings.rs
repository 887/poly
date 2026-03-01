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
pub fn SettingsPage() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let section = app_state.read().settings_section;
    // Subscribe to locale signal so nav labels re-render on language change.
    let _locale = crate::i18n::use_locale().read().clone();
    let mut search_text = use_signal(String::new);
    let sf_raw = search_text.read().clone();
    let sf = sf_raw.to_lowercase();
    // Helper: is this nav item visible given the current search filter?
    let shows = |label: &str| -> bool { sf.is_empty() || label.to_lowercase().contains(&sf) };

    rsx! {
        div { class: "settings-page",
            // Settings navigation
            nav { class: "settings-nav",
                // Search bar
                div { class: "settings-search-bar",
                    input {
                        r#type: "text",
                        class: "settings-search-input",
                        placeholder: "{t(\"settings-search\")}",
                        value: "{sf_raw}",
                        oninput: move |e| search_text.set(e.value()),
                    }
                    if !sf_raw.is_empty() {
                        button {
                            class: "settings-search-clear",
                            onclick: move |_| search_text.set(String::new()),
                            "×"
                        }
                    }
                }
                if shows(&t("settings-voice-video")) {
                    SettingsNavItem {
                        label: t("settings-voice-video"),
                        active: section == SettingsSection::VoiceVideo,
                        onclick: move |_| app_state.write().settings_section = SettingsSection::VoiceVideo,
                    }
                }
                if shows(&t("settings-notifications")) {
                    SettingsNavItem {
                        label: t("settings-notifications"),
                        active: section == SettingsSection::Notifications,
                        onclick: move |_| app_state.write().settings_section = SettingsSection::Notifications,
                    }
                }
                if shows(&t("settings-accounts")) {
                    SettingsNavItem {
                        label: t("settings-accounts"),
                        active: section == SettingsSection::Accounts,
                        onclick: move |_| app_state.write().settings_section = SettingsSection::Accounts,
                    }
                }
                if shows(&t("settings-backup")) {
                    SettingsNavItem {
                        label: t("settings-backup"),
                        active: section == SettingsSection::Backup,
                        onclick: move |_| app_state.write().settings_section = SettingsSection::Backup,
                    }
                }
                if shows(&t("settings-identity")) {
                    SettingsNavItem {
                        label: t("settings-identity"),
                        active: section == SettingsSection::Identity,
                        onclick: move |_| app_state.write().settings_section = SettingsSection::Identity,
                    }
                }
                if shows(&t("settings-theme")) {
                    SettingsNavItem {
                        label: t("settings-theme"),
                        active: section == SettingsSection::Theme,
                        onclick: move |_| app_state.write().settings_section = SettingsSection::Theme,
                    }
                }
                if shows(&t("settings-language")) {
                    SettingsNavItem {
                        label: t("settings-language"),
                        active: section == SettingsSection::Language,
                        onclick: move |_| app_state.write().settings_section = SettingsSection::Language,
                    }
                }
                if shows(&t("settings-general")) {
                    SettingsNavItem {
                        label: t("settings-general"),
                        active: section == SettingsSection::General,
                        onclick: move |_| app_state.write().settings_section = SettingsSection::General,
                    }
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
                    // Appearance merged into Theme — redirect transparently.
                    SettingsSection::Appearance => rsx! {
                        ThemeSettings {}
                    },
                    SettingsSection::General => rsx! {
                        GeneralSettings {}
                    },
                    SettingsSection::VoiceVideo => rsx! {
                        VoiceVideoSettings {}
                    },
                    SettingsSection::Notifications => rsx! {
                        NotificationsSettings {}
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

/// Probe status for step 1 of the add-server wizard.
///
/// Returned by [`crate::sync::probe_server`] and stored in a [`Signal`] so the
/// UI can reactively reflect the connection-check result.
// DECISION(DX-BACKUP-UI-2): Two-step wizard replaces the flat form.
// ProbeStatus stores structured server info returned by GET /api/info.
#[derive(Clone, PartialEq)]
enum ProbeStatus {
    Idle,
    Checking,
    Ready {
        server_name: String,
        password_required: bool,
        registrations_open: bool,
    },
    Error(String),
}

/// Authentication status for step 2 of the add-server wizard.
#[derive(Clone, PartialEq)]
enum WizardAuthStatus {
    Idle,
    Checking,
    Error(String),
}

// BackupSettings — two-step wizard UI for adding servers and per-server sync.
// Step 1: Enter server URL → probe /api/info → see server name + policies.
// Step 2: Set server label (pre-filled) + passphrase (if required) → authenticate.
// Configured servers show status chips, last-sync time, and Sync Now / Re-auth / Remove
// action buttons. On first render a background pull checks for remote changes.
// ── BackupSettings — async helpers ───────────────────────────────────────────

/// Pull from all enabled servers on first mount to catch up on remote changes.
async fn backup_startup_sync(mut servers: Signal<Vec<crate::storage::BackupServerRecord>>) {
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(mut list) = storage.get_backup_servers().await else {
        return;
    };
    for rec in list.iter_mut() {
        if !rec.enabled || rec.token.is_none() {
            continue;
        }
        let Some(ref token) = rec.token.clone() else {
            continue;
        };
        let cfg = crate::sync::BackupServerConfig {
            url: rec.url.clone(),
            name: rec.label.clone(),
            token: Some(token.clone()),
            last_sequence: rec.last_sequence,
        };
        let client = crate::sync::SyncClient::new(cfg);
        if let Ok(blobs) = client.pull(rec.last_sequence).await
            && let Some(latest) = blobs.last()
        {
            let new_seq = u64::try_from(latest.sequence).unwrap_or(rec.last_sequence);
            if new_seq > rec.last_sequence {
                rec.last_sequence = new_seq;
                rec.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
            }
        }
    }
    for rec in &list {
        if let Err(e) = storage.upsert_backup_server(rec).await {
            tracing::error!("Save startup sync state: {e}");
        }
    }
    servers.set(list);
}

/// Encrypt current app settings and push them to one backup server.
///
/// Manages the status chip for `url` throughout: Syncing → Connected or error.
async fn backup_sync_now(
    url: String,
    servers: Signal<Vec<crate::storage::BackupServerRecord>>,
    statuses: Signal<HashMap<String, String>>,
) {
    let mut st = statuses;
    st.write()
        .insert(url.clone(), t("settings-backup-status-syncing"));
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(list) = storage.get_backup_servers().await else {
        return;
    };
    let Some(rec) = list.into_iter().find(|r| r.url == url) else {
        return;
    };
    let cfg = crate::sync::BackupServerConfig {
        url: rec.url.clone(),
        name: rec.label.clone(),
        token: rec.token.clone(),
        last_sequence: rec.last_sequence,
    };
    let client = crate::sync::SyncClient::new(cfg);
    let Ok(settings) = storage.get_app_settings().await else {
        st.write()
            .insert(url, t("settings-backup-status-unreachable"));
        return;
    };
    let Ok(payload) = serde_json::to_vec(&settings) else {
        st.write()
            .insert(url, t("settings-backup-status-unreachable"));
        return;
    };
    let key = storage
        .get_identity_key()
        .await
        .ok()
        .flatten()
        .unwrap_or([0u8; 32]);
    let Ok(enc) = crate::crypto::encrypt(&payload, &key) else {
        st.write()
            .insert(url, t("settings-backup-status-unreachable"));
        return;
    };
    match client.push(&enc).await {
        Ok(seq) => {
            let now = chrono::Utc::now().to_rfc3339();
            if let Ok(mut fresh) = storage.get_backup_servers().await
                && let Some(r) = fresh.iter_mut().find(|r| r.url == url)
            {
                r.last_sequence = seq;
                r.last_synced_at = Some(now);
                let upd = r.clone();
                if storage.upsert_backup_server(&upd).await.is_ok()
                    && let Ok(reload) = storage.get_backup_servers().await
                {
                    let mut srv = servers;
                    srv.set(reload);
                }
            }
            st.write()
                .insert(url, t("settings-backup-status-connected"));
        }
        Err(e) => {
            let msg = if e.to_string().contains("token") {
                t("settings-backup-status-auth-required")
            } else {
                t("settings-backup-status-unreachable")
            };
            tracing::warn!("Sync push error: {e}");
            st.write().insert(url, msg);
        }
    }
}

/// Remove a backup server and reload the list.
async fn backup_remove_server(
    url: String,
    servers: Signal<Vec<crate::storage::BackupServerRecord>>,
) {
    let mut srv = servers;
    let Some(s) = crate::STORAGE.get() else {
        return;
    };
    if let Err(e) = s.remove_backup_server(&url).await {
        tracing::error!("Remove server: {e}");
    }
    if let Ok(list) = s.get_backup_servers().await {
        srv.set(list);
    }
}

/// Re-authenticate with an existing server using a new passphrase.
///
/// Returns `Ok(())` and reloads the server list on success, or `Err(message)`.
async fn backup_reauth(
    url: String,
    passphrase: String,
    servers: Signal<Vec<crate::storage::BackupServerRecord>>,
    reauth_sig: Signal<Option<String>>,
) -> Result<(), String> {
    let Some(storage) = crate::STORAGE.get() else {
        return Err("Storage unavailable".to_string());
    };
    let Ok(settings) = storage.get_app_settings().await else {
        return Err("Failed to load settings".to_string());
    };
    let cfg = crate::sync::BackupServerConfig {
        url: url.clone(),
        name: String::new(),
        token: None,
        last_sequence: 0,
    };
    let mut client = crate::sync::SyncClient::new(cfg);
    let auth = client
        .authenticate(&passphrase, &settings.account_id, "Poly")
        .await
        .map_err(|e| e.to_string())?;
    let Ok(mut list) = storage.get_backup_servers().await else {
        return Err("Failed to reload servers".to_string());
    };
    if let Some(srv) = list.iter_mut().find(|r| r.url == url) {
        srv.token = Some(auth.token);
        srv.token_expires_at = auth.expires_at;
        let upd = srv.clone();
        storage
            .upsert_backup_server(&upd)
            .await
            .map_err(|e| e.to_string())?;
    }
    if let Ok(fresh) = storage.get_backup_servers().await {
        let mut srv = servers;
        srv.set(fresh);
    }
    let mut r = reauth_sig;
    r.set(None);
    Ok(())
}

/// Build and persist a new [`BackupServerRecord`] after authenticating.
///
/// Returns the refreshed server list on success.
async fn wizard_authenticate(
    url: String,
    name: String,
    passphrase: String,
    pass_required: bool,
) -> Result<Vec<crate::storage::BackupServerRecord>, String> {
    let storage = crate::STORAGE
        .get()
        .ok_or_else(|| "Storage unavailable".to_string())?;
    let settings = storage
        .get_app_settings()
        .await
        .map_err(|e| format!("Failed to load settings: {e}"))?;
    let cfg = crate::sync::BackupServerConfig {
        url: url.clone(),
        name: name.clone(),
        token: None,
        last_sequence: 0,
    };
    let mut client = crate::sync::SyncClient::new(cfg);
    let auth_phrase = if pass_required {
        passphrase
    } else {
        String::new()
    };
    let auth = client
        .authenticate(&auth_phrase, &settings.account_id, "Poly")
        .await
        .map_err(|e| e.to_string())?;
    let record = crate::storage::BackupServerRecord {
        url: url.clone(),
        label: if name.is_empty() { url } else { name },
        enabled: true,
        last_sequence: 0,
        token: Some(auth.token),
        token_expires_at: auth.expires_at,
        last_synced_at: None,
    };
    storage
        .upsert_backup_server(&record)
        .await
        .map_err(|e| format!("Save error: {e}"))?;
    storage
        .get_backup_servers()
        .await
        .map_err(|e| format!("Reload error: {e}"))
}

// ── BackupSettings — sub-components ──────────────────────────────────────────

/// Renders the probe connection result box for Step 1 of the add‐server wizard.
#[component]
fn ProbeStatusBox(status: ProbeStatus) -> Element {
    match status {
        ProbeStatus::Idle => rsx! {
            div {}
        },
        ProbeStatus::Checking => rsx! {
            div { class: "probe-status-box probe-checking",
                span { class: "probe-spinner" }
                span { "{t(\"settings-backup-checking\")}" }
            }
        },
        ProbeStatus::Ready {
            server_name,
            password_required,
            registrations_open,
        } => rsx! {
            div { class: if registrations_open { "probe-status-box probe-ok" } else { "probe-status-box probe-error" },
                p { class: "probe-server-name", "✓  {server_name}" }
                p { class: "probe-detail",
                    if password_required {
                        "{t(\"settings-backup-password-required\")}"
                    } else {
                        "{t(\"settings-backup-no-password-required\")}"
                    }
                }
                if !registrations_open {
                    p { class: "probe-full-warning", "{t(\"settings-backup-server-full\")}" }
                }
            }
        },
        ProbeStatus::Error(msg) => rsx! {
            div { class: "probe-status-box probe-error", "✗  {msg}" }
        },
    }
}

/// Renders the authentication status box for Step 2 of the add-server wizard.
#[component]
fn WizardAuthStatusBox(status: WizardAuthStatus) -> Element {
    match status {
        WizardAuthStatus::Idle => rsx! {
            div {}
        },
        WizardAuthStatus::Checking => rsx! {
            div { class: "probe-status-box probe-checking",
                span { class: "probe-spinner" }
                span { "{t(\"settings-backup-connecting\")}" }
            }
        },
        WizardAuthStatus::Error(msg) => rsx! {
            div { class: "probe-status-box probe-error", "✗  {msg}" }
        },
    }
}

/// Inline re-authentication form embedded in a [`ServerCard`].
///
/// Manages its own passphrase input and error state. Closes on success or cancel.
#[component]
fn ReauthForm(
    url: String,
    servers: Signal<Vec<crate::storage::BackupServerRecord>>,
    statuses: Signal<HashMap<String, String>>,
    reauth_url_sig: Signal<Option<String>>,
) -> Element {
    let mut pass = use_signal(String::new);
    let mut err = use_signal(String::new);
    rsx! {
        div { class: "server-reauth-form",
            input {
                r#type: "password",
                class: "form-input",
                placeholder: "{t(\"settings-backup-passphrase-label\")}",
                value: "{pass}",
                oninput: move |e| pass.set(e.value()),
            }
            if !err.read().is_empty() {
                div { class: "probe-status-box probe-error", "{err}" }
            }
            div { class: "server-reauth-actions",
                button {
                    class: "btn btn-sm btn-primary",
                    onclick: move |_| {
                        let url_inner = url.clone();
                        let pw = pass.read().clone();
                        spawn(async move {
                            err.set(String::new());
                            if let Err(e) = backup_reauth(url_inner, pw, servers, reauth_url_sig).await {
                                tracing::warn!("Re-auth: {e}");
                                err.set(format!("{}: {e}", t("settings-backup-auth-failed")));
                            }
                        });
                    },
                    "{t(\"settings-backup-connect\")}"
                }
                button {
                    class: "btn btn-sm btn-ghost",
                    onclick: move |_| {
                        let mut r = reauth_url_sig;
                        r.set(None);
                    },
                    "{t(\"settings-backup-cancel\")}"
                }
            }
        }
    }
}

/// A card representing one configured backup server.
///
/// Shows status, last-sync time, and Sync Now / Re-auth / Remove actions.
/// The re-auth passphrase form is shown inline when "Re-authenticate" is clicked.
#[component]
fn ServerCard(
    record: crate::storage::BackupServerRecord,
    servers: Signal<Vec<crate::storage::BackupServerRecord>>,
    statuses: Signal<HashMap<String, String>>,
) -> Element {
    let reauth_url_sig: Signal<Option<String>> = use_signal(|| None);
    let url = record.url.clone();
    let url_for_sync = url.clone();
    let url_for_reauth = url.clone();
    let url_for_remove = url.clone();
    let label = record.label.clone();
    let last_synced = record
        .last_synced_at
        .clone()
        .unwrap_or_else(|| t("settings-backup-never-synced"));
    let status_text = statuses.read().get(&url).cloned().unwrap_or_else(|| {
        if record.token.is_some() {
            t("settings-backup-status-connected")
        } else {
            t("settings-backup-status-auth-required")
        }
    });
    let connected = status_text == t("settings-backup-status-connected");
    let syncing = status_text == t("settings-backup-status-syncing");
    let status_class = if connected {
        "status-chip status-connected"
    } else if syncing {
        "status-chip status-syncing"
    } else {
        "status-chip status-disconnected"
    };
    let is_reauthenticating = reauth_url_sig.read().as_deref() == Some(url.as_str());
    rsx! {
        div { class: "server-card", key: "{url}",
            div { class: "server-card-header",
                div { class: "server-card-info",
                    span { class: "server-card-name", "{label}" }
                    span { class: "server-card-url", "{url}" }
                }
                div { class: "server-card-meta",
                    span { class: "{status_class}", "{status_text}" }
                    span { class: "server-last-synced", "{last_synced}" }
                }
            }
            div { class: "server-card-actions",
                button {
                    class: "btn btn-sm btn-secondary",
                    onclick: move |_| {
                        let u = url_for_sync.clone();
                        spawn(async move {
                            backup_sync_now(u, servers, statuses).await;
                        });
                    },
                    "{t(\"settings-backup-sync-now\")}"
                }
                button {
                    class: "btn btn-sm btn-secondary",
                    onclick: move |_| {
                        let mut r = reauth_url_sig;
                        r.set(Some(url_for_reauth.clone()));
                    },
                    "{t(\"settings-backup-reauth\")}"
                }
                button {
                    class: "btn btn-sm btn-danger",
                    onclick: move |_| {
                        let u = url_for_remove.clone();
                        spawn(async move {
                            backup_remove_server(u, servers).await;
                        });
                    },
                    "{t(\"settings-backup-remove\")}"
                }
            }
            if is_reauthenticating {
                ReauthForm {
                    url: url.clone(),
                    servers,
                    statuses,
                    reauth_url_sig,
                }
            }
        }
    }
}

/// Step 1 of the add-server wizard: URL entry and server probe.
#[component]
fn WizardStep1(
    wizard_url: Signal<String>,
    probe_status: Signal<ProbeStatus>,
    wizard_step: Signal<u8>,
    wizard_name: Signal<String>,
    wizard_pass_required: Signal<bool>,
    auth_status: Signal<WizardAuthStatus>,
) -> Element {
    rsx! {
        div { class: "wizard-step-body",
            p { class: "wizard-step-hint", "{t(\"settings-backup-step1-hint\")}" }
            div { class: "form-field",
                label { class: "form-label", "{t(\"settings-backup-url-label\")}" }
                div { class: "url-check-row",
                    input {
                        r#type: "text",
                        class: "form-input",
                        placeholder: "{t(\"settings-backup-url-placeholder\")}",
                        value: "{wizard_url}",
                        oninput: move |e| {
                            let mut wu = wizard_url;
                            wu.set(e.value());
                            let mut ps = probe_status;
                            ps.set(ProbeStatus::Idle);
                        },
                    }
                    button {
                        class: "btn btn-secondary",
                        disabled: matches!(*probe_status.read(), ProbeStatus::Checking),
                        onclick: move |_| {
                            let url = wizard_url.read().trim().to_string();
                            if url.is_empty() {
                                let mut ps = probe_status;
                                ps.set(ProbeStatus::Error(t("settings-backup-url-empty")));
                                return;
                            }
                            let mut ps = probe_status;
                            ps.set(ProbeStatus::Checking);
                            spawn(async move {
                                match crate::sync::probe_server(&url).await {
                                    Ok(info) => {
                                        ps.set(ProbeStatus::Ready {
                                            server_name: info.name,
                                            password_required: info.password_required,
                                            registrations_open: info.registrations_open,
                                        })
                                    }
                                    Err(e) => ps.set(ProbeStatus::Error(e.to_string())),
                                }
                            });
                        },
                        "{t(\"settings-backup-check-btn\")}"
                    }
                }
            }
            ProbeStatusBox { status: probe_status.read().clone() }
            div { class: "wizard-actions",
                button {
                    class: "btn btn-ghost",
                    onclick: move |_| {
                        let mut ws = wizard_step;
                        ws.set(0);
                    },
                    "{t(\"settings-backup-cancel\")}"
                }
                button {
                    class: "btn btn-primary",
                    disabled: !matches!(*probe_status.read(), ProbeStatus::Ready { registrations_open: true, .. }),
                    onclick: move |_| {
                        if let ProbeStatus::Ready { server_name, password_required, .. } = probe_status
                            .read()
                            .clone()
                        {
                            let mut wn = wizard_name;
                            wn.set(server_name);
                            let mut wpr = wizard_pass_required;
                            wpr.set(password_required);
                            let mut auth = auth_status;
                            auth.set(WizardAuthStatus::Idle);
                            let mut ws = wizard_step;
                            ws.set(2);
                        }
                    },
                    "{t(\"settings-backup-continue\")}"
                }
            }
        }
    }
}

/// Step 2 of the add-server wizard: name, optional passphrase, and authenticate.
#[component]
fn WizardStep2(
    wizard_step: Signal<u8>,
    wizard_url: Signal<String>,
    wizard_name: Signal<String>,
    wizard_pass: Signal<String>,
    wizard_pass_required: Signal<bool>,
    servers: Signal<Vec<crate::storage::BackupServerRecord>>,
) -> Element {
    let mut auth_status: Signal<WizardAuthStatus> = use_signal(|| WizardAuthStatus::Idle);
    rsx! {
        div { class: "wizard-step-body",
            p { class: "wizard-step-hint", "{t(\"settings-backup-step2-hint\")}" }
            div { class: "form-field",
                label { class: "form-label", "{t(\"settings-backup-label-label\")}" }
                input {
                    r#type: "text",
                    class: "form-input",
                    value: "{wizard_name}",
                    oninput: move |e| {
                        let mut wn = wizard_name;
                        wn.set(e.value());
                    },
                }
            }
            if *wizard_pass_required.read() {
                div { class: "form-field",
                    label { class: "form-label", "{t(\"settings-backup-passphrase-label\")}" }
                    input {
                        r#type: "password",
                        class: "form-input",
                        value: "{wizard_pass}",
                        oninput: move |e| {
                            let mut wp = wizard_pass;
                            wp.set(e.value());
                        },
                    }
                }
            }
            WizardAuthStatusBox { status: auth_status.read().clone() }
            div { class: "wizard-actions",
                button {
                    class: "btn btn-ghost",
                    onclick: move |_| {
                        let mut ws = wizard_step;
                        ws.set(1);
                        auth_status.set(WizardAuthStatus::Idle);
                    },
                    "{t(\"settings-backup-back\")}"
                }
                button {
                    class: "btn btn-primary",
                    disabled: matches!(*auth_status.read(), WizardAuthStatus::Checking),
                    onclick: move |_| {
                        let url = wizard_url.read().trim().to_string();
                        let name = wizard_name.read().trim().to_string();
                        let pass = wizard_pass.read().clone();
                        let pass_req = *wizard_pass_required.read();
                        auth_status.set(WizardAuthStatus::Checking);
                        spawn(async move {
                            match wizard_authenticate(url, name, pass, pass_req).await {
                                Ok(list) => {
                                    let mut srv = servers;
                                    srv.set(list);
                                    let mut ws = wizard_step;
                                    ws.set(0);
                                    auth_status.set(WizardAuthStatus::Idle);
                                }
                                Err(e) => auth_status.set(WizardAuthStatus::Error(e)),
                            }
                        });
                    },
                    "{t(\"settings-backup-finish\")}"
                }
            }
        }
    }
}

/// Add-server wizard (button + two-step flow).
///
/// Step 0: shows the "Add Server" button.
/// Step 1: URL entry and server probe.
/// Step 2: name / passphrase and authenticate.
// DECISION(DX-BACKUP-UI-2): Two-step wizard + background startup sync.
#[component]
fn AddServerWizard(servers: Signal<Vec<crate::storage::BackupServerRecord>>) -> Element {
    let mut wizard_step = use_signal(|| 0u8);
    let mut wizard_url = use_signal(|| "https://".to_string());
    let probe_status: Signal<ProbeStatus> = use_signal(|| ProbeStatus::Idle);
    let mut wizard_name = use_signal(String::new);
    let mut wizard_pass = use_signal(String::new);
    let wizard_pass_required = use_signal(|| false);
    let mut auth_status: Signal<WizardAuthStatus> = use_signal(|| WizardAuthStatus::Idle);
    if *wizard_step.read() == 0 {
        rsx! {
            button {
                class: "btn btn-primary",
                onclick: move |_| {
                    wizard_step.set(1);
                    wizard_url.set("https://".to_string());
                    let mut ps = probe_status;
                    ps.set(ProbeStatus::Idle);
                    wizard_name.set(String::new());
                    wizard_pass.set(String::new());
                    auth_status.set(WizardAuthStatus::Idle);
                },
                "{t(\"settings-backup-add-server\")}"
            }
        }
    } else {
        rsx! {
            div { class: "backup-wizard-card",
                div { class: "wizard-steps",
                    div { class: if *wizard_step.read() >= 1 { "wizard-step active" } else { "wizard-step" },
                        span { class: "wizard-step-num", "1" }
                        span { class: "wizard-step-label", "{t(\"settings-backup-wizard-step1\")}" }
                    }
                    div { class: "wizard-step-divider" }
                    div { class: if *wizard_step.read() >= 2 { "wizard-step active" } else { "wizard-step" },
                        span { class: "wizard-step-num", "2" }
                        span { class: "wizard-step-label", "{t(\"settings-backup-wizard-step2\")}" }
                    }
                }
                if *wizard_step.read() == 1 {
                    WizardStep1 {
                        wizard_url,
                        probe_status,
                        wizard_step,
                        wizard_name,
                        wizard_pass_required,
                        auth_status,
                    }
                }
                if *wizard_step.read() == 2 {
                    WizardStep2 {
                        wizard_step,
                        wizard_url,
                        wizard_name,
                        wizard_pass,
                        wizard_pass_required,
                        servers,
                    }
                }
            }
        }
    }
}

// ── BackupSettings — top-level component ─────────────────────────────────────

/// Backup servers settings section.
///
/// Loads the server list on mount and pulls remote changes in the background,
/// then delegates rendering to [`ServerCard`] (per server) and [`AddServerWizard`].
// DECISION(DX-BACKUP-UI-2): Two-step wizard + background startup sync.
#[component]
fn BackupSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let servers: Signal<Vec<crate::storage::BackupServerRecord>> = use_signal(Vec::new);
    let sync_statuses: Signal<HashMap<String, String>> = use_signal(HashMap::new);

    // Load servers on mount + pull from enabled servers to catch up on remote changes.
    use_effect(move || {
        spawn(async move {
            backup_startup_sync(servers).await;
        });
    });

    rsx! {
        div { class: "settings-section",
            div { class: "settings-section-header",
                h2 { "{t(\"settings-backup\")}" }
                p { class: "settings-section-desc", "{t(\"settings-backup-description\")}" }
            }
            div { class: "server-list",
                if servers.read().is_empty() {
                    p { class: "no-servers-hint", "{t(\"settings-backup-no-servers\")}" }
                }
                for rec in servers.read().clone() {
                    ServerCard { record: rec, servers, statuses: sync_statuses }
                }
            }
            AddServerWizard { servers }
        }
    }
}

// ── IdentitySettings — helpers and sub-components ───────────────────────────

/// Load the user's 24-word mnemonic from storage.
async fn load_mnemonic_words() -> Result<Vec<String>, String> {
    let s = crate::STORAGE
        .get()
        .ok_or_else(|| "Storage not ready".to_string())?;
    let key_bytes = s
        .get_identity_key()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No identity key in storage".to_string())?;
    let identity = crate::crypto::Identity::from_private_key_bytes(&key_bytes);
    let phrase = identity.to_mnemonic().map_err(|e| e.to_string())?;
    Ok(phrase.split_whitespace().map(str::to_string).collect())
}

/// Modal overlay that displays and allows copying the 24-word recovery phrase.
#[component]
fn MnemonicModal(mnemonic_words: Signal<Vec<String>>, show: Signal<bool>) -> Element {
    let mut visible = show;
    rsx! {
        div { class: "modal-overlay", onclick: move |_| visible.set(false),
            div {
                class: "modal-content",
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
                        onclick: move |_| visible.set(false),
                        "{t(\"settings-identity-close\")}"
                    }
                }
            }
        }
    }
}

/// Identity settings section.
///
/// Displays the user's Ed25519 public key (Account ID) and provides a
/// "Show Recovery Phrase" button that opens a [`MnemonicModal`].
#[component]
fn IdentitySettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let mut account_id = use_signal(String::new);
    let mut show_phrase_modal = use_signal(|| false);
    let mut mnemonic_words: Signal<Vec<String>> = use_signal(Vec::new);
    let mut status_msg = use_signal(String::new);

    use_future(move || async move {
        if let Some(s) = crate::STORAGE.get() {
            match s.get_app_settings().await {
                Ok(settings) if !settings.account_id.is_empty() => {
                    account_id.set(settings.account_id);
                }
                Ok(_) => status_msg.set(t("settings-identity-no-identity")),
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
                        spawn(async move {
                            match load_mnemonic_words().await {
                                Ok(words) => {
                                    mnemonic_words.set(words);
                                    show_phrase_modal.set(true);
                                }
                                Err(e) => tracing::error!("Mnemonic: {e}"),
                            }
                        });
                    },
                    "{t(\"settings-identity-show-phrase\")}"
                }
            } else {
                p { class: "settings-info", "{status_msg}" }
            }

            if *show_phrase_modal.read() {
                MnemonicModal { mnemonic_words, show: show_phrase_modal }
            }
        }
    }
}

/// Theme settings section.
///
/// Reads/writes the `Signal<ThemeConfig>` provided by [`crate::ui::App`].
/// Changing the preset updates the signal immediately (re-renders the
/// `<style id="poly-theme">` in App) and persists to storage.
///
/// Persist the theme config to storage (fire-and-forget helper).
async fn persist_theme(config: ThemeConfig) {
    if let Some(s) = crate::STORAGE.get() {
        if let Err(e) = s.set_theme_config(&config).await {
            tracing::error!("Failed to persist theme config: {e}");
        } else {
            tracing::info!("Theme config persisted ✓");
        }
    }
}

/// Visual preset picker — colored buttons for each built-in theme.
#[component]
fn ThemePresetPicker(theme_config: Signal<ThemeConfig>) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let current = theme_config.read().preset.canonical();
    const PRESETS: &[(ThemePreset, &str, &str)] = &[
        (ThemePreset::Blue, "blue", "theme-blue"),
        (ThemePreset::Purple, "purple", "theme-purple"),
        (ThemePreset::Red, "red", "theme-red"),
        (ThemePreset::Green, "green", "theme-green"),
        (ThemePreset::Monotone, "monotone", "theme-monotone"),
    ];
    rsx! {
        div { class: "theme-section",
            label { class: "settings-label", "{t(\"settings-theme-preset\")}" }
            div { class: "theme-preset-row",
                for (preset , data_name , i18n_key) in PRESETS {
                    {
                        let preset = *preset;
                        let data_name = *data_name;
                        let i18n_key = *i18n_key;
                        let is_active = current == preset;
                        rsx! {
                            button {
                                class: if is_active { "theme-preset-btn active" } else { "theme-preset-btn" },
                                "data-preset": data_name,
                                onclick: move |_| {
                                    let mut cfg = theme_config.read().clone();
                                    cfg.preset = preset;
                                    theme_config.set(cfg.clone());
                                    spawn(async move {
                                        persist_theme(cfg).await;
                                    });
                                },
                                "{t(i18n_key)}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Dark / Light / Follow Device toggle.
#[component]
fn ThemeColorModeSelector(theme_config: Signal<ThemeConfig>) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let current = theme_config.read().color_mode;
    const MODES: &[(crate::theme::ColorMode, &str)] = &[
        (crate::theme::ColorMode::Dark, "settings-dark-mode"),
        (crate::theme::ColorMode::Light, "settings-light-mode"),
        (
            crate::theme::ColorMode::FollowDevice,
            "settings-follow-device",
        ),
    ];
    rsx! {
        div { class: "theme-section",
            label { class: "settings-label", "{t(\"settings-color-mode\")}" }
            div { class: "color-mode-row",
                for (mode , key) in MODES {
                    {
                        let mode = *mode;
                        let key = *key;
                        let is_active = current == mode;
                        rsx! {
                            button {
                                class: if is_active { "btn btn-sm color-mode-btn active" } else { "btn btn-sm color-mode-btn" },
                                onclick: move |_| {
                                    let mut cfg = theme_config.read().clone();
                                    cfg.color_mode = mode;
                                    theme_config.set(cfg.clone());
                                    spawn(async move {
                                        persist_theme(cfg).await;
                                    });
                                },
                                "{t(key)}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Color pickers for the six most impactful CSS variables.
///
/// Picking a color inserts the value into [`ThemeConfig::color_overrides`].
#[component]
fn ThemeColorCustomizer(theme_config: Signal<ThemeConfig>) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    const VARS: &[(&str, &str)] = &[
        ("--accent-primary", "Accent"),
        ("--bg-primary", "Background"),
        ("--bg-surface", "Surface"),
        ("--text-primary", "Text"),
        ("--text-secondary", "Secondary Text"),
        ("--border-primary", "Border"),
    ];
    let config = theme_config.read().clone();
    rsx! {
        div { class: "theme-section",
            label { class: "settings-label", "{t(\"settings-color-overrides\")}" }
            div { class: "color-overrides-grid",
                for (var_name , display_label) in VARS {
                    {
                        let var_name = *var_name;
                        let display_label = *display_label;
                        let cur = config
                            .color_overrides
                            .get(var_name)
                            .cloned()
                            .unwrap_or_else(|| {
                                crate::theme::extract_var_value(
                                        config.preset,
                                        config.color_mode,
                                        var_name,
                                    )
                                    .unwrap_or_else(|| "#808080".to_string())
                            });
                        rsx! {
                            div { class: "color-override-item",
                                label { class: "color-override-label", "{display_label}" }
                                input {
                                    r#type: "color",
                                    class: "color-picker",
                                    value: cur,
                                    oninput: move |e| {
                                        let color = e.value();
                                        let mut cfg = theme_config.read().clone();
                                        cfg.color_overrides.insert(var_name.to_string(), color);
                                        theme_config.set(cfg.clone());
                                        spawn(async move {
                                            persist_theme(cfg).await;
                                        });
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// CSS editor with enable toggle, pre-populated variable template, and
/// import/export controls.
///
/// When disabled (default), the editor is visible but greyed out and
/// the CSS is not injected. The template lists every CSS variable
/// (commented out) so users can see what is available.
#[component]
fn ThemeCssEditor(theme_config: Signal<ThemeConfig>) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let config = theme_config.read().clone();

    // Build the template the very first time (empty custom_css).
    let initial_css = if config.custom_css.is_empty() {
        crate::theme::build_css_template(&config)
    } else {
        config.custom_css.clone()
    };
    let mut local_css = use_signal(|| initial_css);
    let css_enabled = config.custom_css_enabled;

    rsx! {
        div { class: "theme-section",
            // Toggle row
            div { class: "css-toggle-row",
                label { class: "settings-label", "{t(\"settings-theme-custom-css\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: css_enabled,
                        onchange: move |e| {
                            let enabled = e.checked();
                            let mut cfg = theme_config.read().clone();
                            cfg.custom_css_enabled = enabled;
                            theme_config.set(cfg.clone());
                            spawn(async move {
                                persist_theme(cfg).await;
                            });
                        },
                    }
                    span { class: "toggle-slider" }
                }
            }
            p { class: "css-hint", "{t(\"settings-css-hint\")}" }
            textarea {
                class: if css_enabled { "css-editor" } else { "css-editor css-editor-disabled" },
                rows: 14,
                value: local_css.read().clone(),
                oninput: move |e| local_css.set(e.value()),
                onblur: move |_| {
                    let css = local_css.read().clone();
                    let mut cfg = theme_config.read().clone();
                    cfg.custom_css = css;
                    theme_config.set(cfg.clone());
                    spawn(async move {
                        persist_theme(cfg).await;
                    });
                },
            }
            div { class: "theme-actions",
                button {
                    class: "btn btn-secondary",
                    onclick: move |_| {
                        let css = local_css.read().clone();
                        let mut cfg = theme_config.read().clone();
                        cfg.custom_css = css;
                        theme_config.set(cfg.clone());
                        spawn(async move {
                            persist_theme(cfg).await;
                        });
                    },
                    "{t(\"settings-theme-apply-css\")}"
                }
                button {
                    class: "btn btn-secondary",
                    onclick: move |_| {
                        let exported = crate::theme::export_theme(&theme_config.read());
                        let js = format!(
                            "navigator.clipboard.writeText({:?}).catch(()=>{{}})",
                            exported,
                        );
                        let _ = document::eval(&js);
                    },
                    "{t(\"settings-theme-export\")}"
                }
                button {
                    class: "btn btn-secondary",
                    onclick: move |_| {
                        spawn(async move {
                            let mut eval = document::eval(
                                "navigator.clipboard.readText().then(t=>dioxus.send(t)).catch(()=>dioxus.send(''))",
                            );
                            if let Ok(val) = eval.recv::<serde_json::Value>().await
                                && let Some(s) = val.as_str()
                            {
                                let imported = crate::theme::import_theme(s);
                                local_css
                                    .set(
                                        if imported.custom_css.is_empty() {
                                            crate::theme::build_css_template(&imported)
                                        } else {
                                            imported.custom_css.clone()
                                        },
                                    );
                                theme_config.set(imported.clone());
                                persist_theme(imported).await;
                            }
                        });
                    },
                    "{t(\"settings-theme-import\")}"
                }
                button {
                    class: "btn btn-secondary",
                    onclick: move |_| {
                        let template = crate::theme::build_css_template(&theme_config.read());
                        local_css.set(template);
                    },
                    "{t(\"settings-css-reset-template\")}"
                }
            }
        }
    }
}

/// Theme settings page — presets, color mode, color overrides, and CSS editor.
///
/// Replaces the separate Appearance page: everything color/theme related
/// is now in one place.
#[component]
fn ThemeSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let theme_config = use_context::<Signal<ThemeConfig>>();
    rsx! {
        div { class: "settings-section theme-settings",
            h2 { "{t(\"settings-theme\")}" }
            p { class: "settings-description", "{t(\"settings-theme-description\")}" }
            ThemePresetPicker { theme_config }
            ThemeColorModeSelector { theme_config }
            ThemeColorCustomizer { theme_config }
            ThemeCssEditor { theme_config }
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

#[derive(Clone, Copy)]
enum ResetKind {
    User,
    Nuke,
}

// DECISION(DX-2.5.1): Reset flow uses ClientManager context so all active
// backends can be logged out before storage is wiped.
async fn run_reset_flow(
    kind: ResetKind,
    mut client_manager: Signal<crate::client_manager::ClientManager>,
    mut chat_data: Signal<crate::state::ChatData>,
    mut app_state: Signal<AppState>,
) -> Result<(), String> {
    let account_ids = client_manager.read().active_account_ids();
    for account_id in account_ids {
        let backend = client_manager.read().get_backend(&account_id);
        if let Some(backend_handle) = backend {
            let mut guard = backend_handle.write().await;
            if let Err(err) = guard.logout().await {
                tracing::warn!("Logout failed for account {account_id}: {err}");
            }
        }
    }
    client_manager.write().clear_all_backends();

    chat_data.set(crate::state::ChatData::default());
    let nav = crate::state::NavigationState {
        view: crate::state::View::Setup,
        ..Default::default()
    };
    {
        let mut state = app_state.write();
        state.is_setup_complete = false;
        state.nav = nav;
    }

    let Some(storage) = crate::STORAGE.get() else {
        return Err(t("settings-reset-error-no-storage"));
    };

    match kind {
        ResetKind::User => storage
            .reset_user_data()
            .await
            .map_err(|e| format!("{}: {e}", t("settings-reset-error-failed")))?,
        ResetKind::Nuke => storage
            .nuke_all_data()
            .await
            .map_err(|e| format!("{}: {e}", t("settings-nuke-error-failed")))?,
    }

    document::eval("window.location.reload();");
    Ok(())
}

/// General settings section.
#[component]
fn GeneralSettings() -> Element {
    let app_state: Signal<AppState> = use_context();
    let _locale = crate::i18n::use_locale().read().clone();
    let client_manager: Signal<crate::client_manager::ClientManager> = use_context();
    let chat_data: Signal<crate::state::ChatData> = use_context();
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-general\")}" }
            p { class: "settings-description", "{t(\"settings-general-description\")}" }
            div { class: "general-reset-actions",
                p { class: "settings-description", "{t(\"settings-reset-description\")}" }
                button {
                    class: "btn btn-danger",
                    disabled: *busy.read(),
                    onclick: move |_| {
                        if *busy.read() {
                            return;
                        }
                        busy.set(true);
                        error.set(String::new());
                        spawn(async move {
                            if let Err(err) = run_reset_flow(
                                    ResetKind::User,
                                    client_manager,
                                    chat_data,
                                    app_state,
                                )
                                .await
                            {
                                error.set(err);
                                busy.set(false);
                            }
                        });
                    },
                    "{t(\"settings-reset-app\")}"
                }
                button {
                    class: "btn btn-warning btn-nuke",
                    disabled: *busy.read(),
                    onclick: move |_| {
                        if *busy.read() {
                            return;
                        }
                        busy.set(true);
                        error.set(String::new());
                        spawn(async move {
                            if let Err(err) = run_reset_flow(
                                    ResetKind::Nuke,
                                    client_manager,
                                    chat_data,
                                    app_state,
                                )
                                .await
                            {
                                error.set(err);
                                busy.set(false);
                            }
                        });
                    },
                    "☢️ {t(\"settings-nuke-app\")}"
                }
                if !error.read().is_empty() {
                    p { class: "general-reset-error", "{error.read()}" }
                }
            }
                // TODO(phase-2.7.9.10): Notification preferences, startup behavior
        }
    }
}
/// Voice & Video settings section.
///
/// Lets the user configure audio/video input/output devices, volume levels,
/// voice activity detection mode, noise suppression and echo cancellation.
/// (Actual device enumeration uses browser/OS APIs wired up in later phases.)
#[component]
fn VoiceVideoSettings() -> Element {
    let mut input_vol = use_signal(|| 80_u32);
    let mut output_vol = use_signal(|| 80_u32);
    let mut vad_mode = use_signal(|| "vad"); // "vad" | "ptt"
    let mut noise_suppress = use_signal(|| "standard"); // "off" | "standard" | "high"
    let mut echo_cancel = use_signal(|| true);
    let mut mic_testing = use_signal(|| false);

    rsx! {
        div { class: "settings-section voice-settings",
            h2 { "{t(\"settings-voice-video\")}" }

            // Input device
            div { class: "voice-settings-row",
                label { class: "voice-settings-label", "{t(\"voice-input-device\")}" }
                select { class: "poly-select-native",
                    option { value: "default", "Default Microphone" }
                }
            }
            // Input volume
            div { class: "voice-settings-row",
                label { class: "voice-settings-label",
                    "{t(\"voice-input-volume\")} — {input_vol}%"
                }
                input {
                    r#type: "range",
                    class: "voice-settings-slider",
                    min: "0",
                    max: "100",
                    value: "{input_vol}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<u32>() {
                            input_vol.set(v);
                        }
                    },
                }
            }
            // Mic test
            div { class: "voice-settings-row",
                button {
                    class: if *mic_testing.read() { "mic-test-btn active" } else { "mic-test-btn" },
                    onclick: move |_| {
                        let current = *mic_testing.read();
                        mic_testing.set(!current);
                    },
                    if *mic_testing.read() { "{t(\"voice-mic-test-stop\")}" } else { "{t(\"voice-mic-test\")}" }
                }
                if *mic_testing.read() {
                    div { class: "mic-level-bar",
                        div { class: "mic-level-fill", style: "width: 40%;" }
                    }
                }
            }

            // Output device
            div { class: "voice-settings-row",
                label { class: "voice-settings-label", "{t(\"voice-output-device\")}" }
                select { class: "poly-select-native",
                    option { value: "default", "Default Speakers" }
                }
            }
            // Output volume
            div { class: "voice-settings-row",
                label { class: "voice-settings-label",
                    "{t(\"voice-output-volume\")} — {output_vol}%"
                }
                input {
                    r#type: "range",
                    class: "voice-settings-slider",
                    min: "0",
                    max: "100",
                    value: "{output_vol}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<u32>() {
                            output_vol.set(v);
                        }
                    },
                }
            }

            // Voice Activity Detection vs Push-to-Talk
            div { class: "voice-settings-row voice-mode-row",
                label { class: "voice-settings-label", "{t(\"voice-input-mode\")}" }
                div { class: "voice-mode-options",
                    label { class: "voice-mode-option",
                        input {
                            r#type: "radio",
                            name: "voice-mode",
                            value: "vad",
                            checked: *vad_mode.read() == "vad",
                            onchange: move |_| vad_mode.set("vad"),
                        }
                        "{t(\"voice-input-vad\")}"
                    }
                    label { class: "voice-mode-option",
                        input {
                            r#type: "radio",
                            name: "voice-mode",
                            value: "ptt",
                            checked: *vad_mode.read() == "ptt",
                            onchange: move |_| vad_mode.set("ptt"),
                        }
                        "{t(\"voice-input-ptt\")}"
                    }
                }
            }

            // Noise suppression
            div { class: "voice-settings-row",
                label { class: "voice-settings-label", "{t(\"voice-noise-suppression\")}" }
                div { class: "voice-mode-options",
                    for (val , lbl) in [("off", t("voice-noise-off")), ("standard", t("voice-noise-standard")), ("high", t("voice-noise-high"))] {
                        {
                            let val_owned = val;
                            let is_checked = *noise_suppress.read() == val_owned;
                            rsx! {
                                label { class: "voice-mode-option",
                                    input {
                                        r#type: "radio",
                                        name: "noise-suppress",
                                        value: "{val_owned}",
                                        checked: is_checked,
                                        onchange: move |_| noise_suppress.set(val_owned),
                                    }
                                    "{lbl}"
                                }
                            }
                        }
                    }
                }
            }

            // Echo cancellation toggle
            div { class: "voice-settings-row toggle-row",
                label { class: "voice-settings-label", "{t(\"voice-echo-cancel\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *echo_cancel.read(),
                        onchange: move |e| echo_cancel.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
        }
    }
}

/// Notifications settings section.
///
/// Controls desktop notification permission, per-event notification
/// toggles, sound preferences and badge visibility.
/// The actual permission request uses the Web Notifications API (on web)
/// and OS notifications on desktop — wired up in later phases.
#[component]
fn NotificationsSettings() -> Element {
    let mut desktop_notifs = use_signal(|| false);
    let mut notif_streams = use_signal(|| true);
    let mut notif_friends_voice = use_signal(|| true);
    let mut notif_reactions = use_signal(|| true);
    let mut sound_new_msg = use_signal(|| true);
    let mut sound_dm = use_signal(|| true);
    let mut sound_ring = use_signal(|| true);
    let mut badge_unread = use_signal(|| true);

    rsx! {
        div { class: "settings-section notif-settings",
            h2 { "{t(\"settings-notifications\")}" }

            // Desktop notification permission
            div { class: "notif-toggle-row notif-permission-row",
                div { class: "notif-toggle-label",
                    span { class: "notif-toggle-title", "{t(\"notif-enable-desktop\")}" }
                    span { class: "notif-toggle-desc",
                        "Requires browser / OS permission"
                    }
                }
                div { class: "notif-permission-controls",
                    label { class: "toggle-switch",
                        input {
                            r#type: "checkbox",
                            checked: *desktop_notifs.read(),
                            onchange: move |e| desktop_notifs.set(e.checked()),
                        }
                        span { class: "toggle-slider" }
                    }
                    if !*desktop_notifs.read() {
                        button {
                            class: "btn btn-primary btn-sm",
                            onclick: move |_| {
                                // TODO(phase-3): call Notification.requestPermission() via JS eval
                                desktop_notifs.set(true);
                            },
                            "{t(\"notif-permission-request\")}"
                        }
                    }
                }
            }

            h3 { class: "notif-section-header", "Notify me about" }

            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-streams\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *notif_streams.read(),
                        onchange: move |e| notif_streams.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-friends-voice\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *notif_friends_voice.read(),
                        onchange: move |e| notif_friends_voice.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-reactions\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *notif_reactions.read(),
                        onchange: move |e| notif_reactions.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }

            h3 { class: "notif-section-header", "Sounds" }

            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-sounds-new-message\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *sound_new_msg.read(),
                        onchange: move |e| sound_new_msg.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-sounds-dm\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *sound_dm.read(),
                        onchange: move |e| sound_dm.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-sounds-ring\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *sound_ring.read(),
                        onchange: move |e| sound_ring.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }

            h3 { class: "notif-section-header", "Badges" }

            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-badge-unread\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *badge_unread.read(),
                        onchange: move |e| badge_unread.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
        }
    }
}
