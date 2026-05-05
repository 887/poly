//! Per-chat style tool handlers (Phase E).

use crate::memory::MemoryDb;
use serde_json::Value;

use super::{err_result, ok_result, str_arg};

pub(super) fn handle_set_chat_style(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let tone          = str_arg(args, "tone");
    let formality     = str_arg(args, "formality");
    let emoji_allowed = args.get("emoji_allowed").and_then(serde_json::Value::as_bool);
    let signature     = str_arg(args, "signature");
    let extra_notes   = str_arg(args, "extra_notes");
    match mem.set_chat_style(account_id, chat_id, tone, formality, emoji_allowed, signature, extra_notes) {
        Ok(()) => ok_result("style saved"),
        Err(e) => err_result(format!("set_chat_style failed: {e}")),
    }
}

pub(super) fn handle_get_chat_style(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    match mem.get_chat_style(account_id, chat_id) {
        Ok(maybe) => ok_result(serde_json::to_string_pretty(&maybe).unwrap_or_default()),
        Err(e) => err_result(format!("get_chat_style failed: {e}")),
    }
}

pub(super) fn handle_list_chat_styles(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = str_arg(args, "account_id");
    match mem.list_chat_styles(account_id) {
        Ok(list) => ok_result(serde_json::to_string_pretty(&list).unwrap_or_default()),
        Err(e) => err_result(format!("list_chat_styles failed: {e}")),
    }
}

pub(super) fn handle_forget_chat_style(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    match mem.forget_chat_style(account_id, chat_id) {
        Ok(()) => ok_result("style deleted"),
        Err(e) => err_result(format!("forget_chat_style failed: {e}")),
    }
}
