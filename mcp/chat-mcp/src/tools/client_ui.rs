//! Client-provided UI surface handlers (WP 8, plan-client-ui-surface).

use crate::state::BackendPool;
use serde_json::Value;

use super::{err_result, ok_result, parse_cursor_kind, parse_menu_target, parse_settings_scope, str_arg};
use super::chat::find_backend;

use poly_client::{Cursor, MenuTargetKind};

pub(super) async fn handle_context_menu(
    args: &Value,
    pool: &BackendPool,
    target: MenuTargetKind,
) -> Value {
    let target_id = match str_arg(args, "target_id") {
        Some(t) => t,
        None => return err_result("missing 'target_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_context_menu_items(target, target_id).await {
        Ok(items) => ok_result(serde_json::to_string_pretty(&items).unwrap_or_default()),
        Err(e) => err_result(format!("get_context_menu_items failed: {e}")),
    }
}

pub(super) async fn handle_invoke_context_action(args: &Value, pool: &BackendPool) -> Value {
    let action_id = match str_arg(args, "action_id") {
        Some(a) => a,
        None => return err_result("missing 'action_id'"),
    };
    let target_kind_str = match str_arg(args, "target_kind") {
        Some(k) => k,
        None => return err_result("missing 'target_kind'"),
    };
    let target = match parse_menu_target(target_kind_str) {
        Some(t) => t,
        None => return err_result(format!("unknown target_kind: {target_kind_str}")),
    };
    let target_id = match str_arg(args, "target_id") {
        Some(t) => t,
        None => return err_result("missing 'target_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.invoke_context_action(action_id, target, target_id).await {
        Ok(outcome) => ok_result(serde_json::to_string_pretty(&outcome).unwrap_or_default()),
        Err(e) => err_result(format!("invoke_context_action failed: {e}")),
    }
}

pub(super) async fn handle_plugin_settings_sections(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_settings_sections().await {
        Ok(sections) => ok_result(serde_json::to_string_pretty(&sections).unwrap_or_default()),
        Err(e) => err_result(format!("get_settings_sections failed: {e}")),
    }
}

pub(super) async fn handle_plugin_setting_get(args: &Value, pool: &BackendPool) -> Value {
    let scope_str = match str_arg(args, "scope") {
        Some(s) => s,
        None => return err_result("missing 'scope'"),
    };
    let scope = match parse_settings_scope(scope_str) {
        Some(s) => s,
        None => return err_result(format!("unknown scope: {scope_str}")),
    };
    let scope_id = match str_arg(args, "scope_id") {
        Some(s) => s,
        None => return err_result("missing 'scope_id'"),
    };
    let key = match str_arg(args, "key") {
        Some(k) => k,
        None => return err_result("missing 'key'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_setting_value(scope, scope_id, key).await {
        Ok(v) => ok_result(v),
        Err(e) => err_result(format!("get_setting_value failed: {e}")),
    }
}

pub(super) async fn handle_plugin_setting_set(args: &Value, pool: &BackendPool) -> Value {
    let scope_str = match str_arg(args, "scope") {
        Some(s) => s,
        None => return err_result("missing 'scope'"),
    };
    let scope = match parse_settings_scope(scope_str) {
        Some(s) => s,
        None => return err_result(format!("unknown scope: {scope_str}")),
    };
    let scope_id = match str_arg(args, "scope_id") {
        Some(s) => s,
        None => return err_result("missing 'scope_id'"),
    };
    let key = match str_arg(args, "key") {
        Some(k) => k,
        None => return err_result("missing 'key'"),
    };
    let value = match str_arg(args, "value") {
        Some(v) => v,
        None => return err_result("missing 'value'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.set_setting_value(scope, scope_id, key, value).await {
        Ok(()) => ok_result("ok"),
        Err(e) => err_result(format!("set_setting_value failed: {e}")),
    }
}

pub(super) async fn handle_sidebar_declaration(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_sidebar_declaration().await {
        Ok(d) => ok_result(serde_json::to_string_pretty(&d).unwrap_or_default()),
        Err(e) => err_result(format!("get_sidebar_declaration failed: {e}")),
    }
}

pub(super) async fn handle_invoke_sidebar_action(args: &Value, pool: &BackendPool) -> Value {
    let action_id = match str_arg(args, "action_id") {
        Some(a) => a,
        None => return err_result("missing 'action_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.invoke_sidebar_action(action_id).await {
        Ok(outcome) => ok_result(serde_json::to_string_pretty(&outcome).unwrap_or_default()),
        Err(e) => err_result(format!("invoke_sidebar_action failed: {e}")),
    }
}

pub(super) async fn handle_channel_view(args: &Value, pool: &BackendPool) -> Value {
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_channel_view(channel_id).await {
        Ok(d) => ok_result(serde_json::to_string_pretty(&d).unwrap_or_default()),
        Err(e) => err_result(format!("get_channel_view failed: {e}")),
    }
}

pub(super) async fn handle_view_rows(args: &Value, pool: &BackendPool) -> Value {
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let cursor = match (str_arg(args, "cursor_kind"), str_arg(args, "cursor_value")) {
        (Some(kind_s), Some(val)) => match parse_cursor_kind(kind_s) {
            Some(kind) => Some(Cursor { kind, value: val.to_string() }),
            None => return err_result(format!("unknown cursor_kind: {kind_s}")),
        },
        (None, None) => None,
        _ => return err_result("cursor_kind and cursor_value must both be present or both absent"),
    };
    let sort_id = str_arg(args, "sort_id");
    let filter_id = str_arg(args, "filter_id");
    let tab_id = str_arg(args, "tab_id");
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_view_rows(channel_id, cursor, sort_id, filter_id, tab_id).await {
        Ok(page) => ok_result(serde_json::to_string_pretty(&page).unwrap_or_default()),
        Err(e) => err_result(format!("get_view_rows failed: {e}")),
    }
}

pub(super) async fn handle_composer_buttons(args: &Value, pool: &BackendPool) -> Value {
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_composer_buttons(channel_id).await {
        Ok(btns) => ok_result(serde_json::to_string_pretty(&btns).unwrap_or_default()),
        Err(e) => err_result(format!("get_composer_buttons failed: {e}")),
    }
}

pub(super) async fn handle_message_actions(args: &Value, pool: &BackendPool) -> Value {
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let message_id = match str_arg(args, "message_id") {
        Some(m) => m,
        None => return err_result("missing 'message_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_message_actions(channel_id, message_id).await {
        Ok(items) => ok_result(serde_json::to_string_pretty(&items).unwrap_or_default()),
        Err(e) => err_result(format!("get_message_actions failed: {e}")),
    }
}

pub(super) async fn handle_invoke_composer_action(args: &Value, pool: &BackendPool) -> Value {
    let action_id = match str_arg(args, "action_id") {
        Some(a) => a,
        None => return err_result("missing 'action_id'"),
    };
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.invoke_composer_action(action_id, channel_id).await {
        Ok(outcome) => ok_result(serde_json::to_string_pretty(&outcome).unwrap_or_default()),
        Err(e) => err_result(format!("invoke_composer_action failed: {e}")),
    }
}

pub(super) async fn handle_invoke_message_action(args: &Value, pool: &BackendPool) -> Value {
    let action_id = match str_arg(args, "action_id") {
        Some(a) => a,
        None => return err_result("missing 'action_id'"),
    };
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let message_id = match str_arg(args, "message_id") {
        Some(m) => m,
        None => return err_result("missing 'message_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.invoke_message_action(action_id, channel_id, message_id).await {
        Ok(outcome) => ok_result(serde_json::to_string_pretty(&outcome).unwrap_or_default()),
        Err(e) => err_result(format!("invoke_message_action failed: {e}")),
    }
}
