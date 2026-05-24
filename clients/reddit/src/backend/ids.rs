//! ID/name bijections between Reddit's `<type>_<base36>` fullname scheme
//! and Poly's namespaced id strings.
//!
//! All helpers are `pub(crate)` because sibling trait-impl modules
//! reference them across files. Carved out in SOLID-audit-reddit C.3.

pub(crate) fn server_id_for_sub(sub: &str) -> String {
    format!("r_{sub}")
}

pub(crate) fn sub_from_server_id(id: &str) -> Option<&str> {
    id.strip_prefix("r_")
}

pub(crate) fn channel_id_for_sub(sub: &str) -> String {
    format!("c_posts_{sub}")
}

pub(crate) fn sub_from_channel_id(id: &str) -> Option<&str> {
    id.strip_prefix("c_posts_")
}

pub(crate) fn message_id_for_post(post_id: &str) -> String {
    format!("t3_{post_id}")
}

pub(crate) fn message_id_for_dm(dm_id: &str) -> String {
    format!("t4_{dm_id}")
}

pub(crate) fn dm_channel_id_for_dm(dm_id: &str) -> String {
    format!("dm_{dm_id}")
}

pub(crate) fn user_id_for_name(name: &str) -> String {
    format!("u_{name}")
}
