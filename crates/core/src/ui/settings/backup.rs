//! Backup server settings — add servers, sync, re-auth, remove.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
//!
//! ## Architecture
//!
//! The UI flow is a two-step wizard:
//! - **Step 1:** Enter URL → probe `/api/info` → show server name + policies.
//! - **Step 2:** Set label + passphrase → authenticate.
//!
//! Configured servers show status chips, last-sync time, and
//! Sync Now / Re-auth / Remove action buttons.
// DECISION(DX-BACKUP-UI-2): Two-step wizard + background startup sync.

use crate::i18n::t;
use dioxus::prelude::*;
use std::collections::HashMap;
use poly_ui_macros::context_menu;

// ── Supporting types ──────────────────────────────────────────────────────────

/// Probe status for step 1 of the add-server wizard.
///
/// Returned by [`crate::sync::probe_server`] and stored in a [`Signal`] so the
/// UI can reactively reflect the connection-check result.
// DECISION(DX-BACKUP-UI-2): Two-step wizard replaces the flat form.
#[derive(Clone, PartialEq)]
pub(super) enum ProbeStatus {
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
pub(super) enum WizardAuthStatus {
    Idle,
    Checking,
    Error(String),
}

// ── Async helpers ─────────────────────────────────────────────────────────────

/// Pull from all enabled servers on first mount to catch up on remote changes.
pub(super) async fn backup_startup_sync(
    mut servers: Signal<Vec<crate::storage::BackupServerRecord>>,
) {
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
pub(super) async fn backup_sync_now(
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
pub(super) async fn backup_remove_server(
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
pub(super) async fn backup_reauth(
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
pub(super) async fn wizard_authenticate(
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

// ── Sub-components ────────────────────────────────────────────────────────────

/// Renders the probe connection result box for Step 1 of the add-server wizard.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(super) fn ProbeStatusBox(status: ProbeStatus) -> Element {
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(super) fn WizardAuthStatusBox(status: WizardAuthStatus) -> Element {
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(super) fn ReauthForm(
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
                            if let Err(e) =
                                backup_reauth(url_inner, pw, servers, reauth_url_sig).await
                            {
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(super) fn ServerCard(
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(super) fn WizardStep1(
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(super) fn WizardStep2(
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(super) fn AddServerWizard(servers: Signal<Vec<crate::storage::BackupServerRecord>>) -> Element {
    let mut wizard_step = use_signal(|| 0u8);
    let mut wizard_url = use_signal(|| "http://127.0.0.1:8080".to_string());
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
                    wizard_url.set("http://127.0.0.1:8080".to_string());
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

// ── Top-level component ───────────────────────────────────────────────────────

/// Backup servers settings section.
///
/// Loads the server list on mount and pulls remote changes in the background,
/// then delegates rendering to [`ServerCard`] (per server) and [`AddServerWizard`].
// DECISION(DX-BACKUP-UI-2): Two-step wizard + background startup sync.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(super) fn BackupSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let servers: Signal<Vec<crate::storage::BackupServerRecord>> = use_signal(Vec::new);
    let sync_statuses: Signal<HashMap<String, String>> = use_signal(HashMap::new);

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
