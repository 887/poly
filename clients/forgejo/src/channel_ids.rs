//! Channel-ID parsing helpers for the Forgejo backend.
//!
//! All Forgejo channel IDs follow the pattern `fj-{kind}-{owner}/{repo}[-{number}]`.
//! These helpers centralise the parsing so that `lib.rs` impl blocks stay
//! free of string-manipulation detail.

use poly_client::{ClientError, ClientResult};
use poly_common_forge::split_owner_repo;

use crate::mapping;
use crate::ForgejoClient;

/// Extract `(owner, repo)` from an issue thread channel ID.
///
/// Handles `fj-issue-{owner}/{repo}-{number}`.
/// The `/` separates owner from repo; the trailing `-{number}` is stripped first.
pub fn parse_issue_thread_owner_repo(channel_id: &str) -> ClientResult<(String, String)> {
    // Strip "fj-issue-" prefix, then the remaining has "{owner}/{repo}-{number}".
    // rsplitn(2, '-') splits off the trailing issue number first (last '-'),
    // then split_owner_repo splits on '/' to get owner and repo.
    let rest = channel_id
        .strip_prefix("fj-issue-")
        .ok_or_else(|| ClientError::NotFound(format!("not an issue thread channel: {channel_id}")))?;
    // rsplitn(2, '-') → [number, "owner/repo"]
    let parts: Vec<&str> = rest.rsplitn(2, '-').collect();
    match parts.as_slice() {
        [_, owner_repo] => split_owner_repo(owner_repo),
        _ => Err(ClientError::NotFound(format!(
            "malformed issue thread channel: {channel_id}"
        ))),
    }
}

/// Look up `(owner, repo)` strings for a server ID from the in-memory cache.
pub async fn repo_owner_name_from_server_id(
    client: &ForgejoClient,
    server_id: &str,
) -> ClientResult<(String, String)> {
    let full_name = {
        let cache = client.repos.lock().await;
        cache
            .iter()
            .find(|r| mapping::server_id_for_repo(r) == server_id)
            .ok_or_else(|| ClientError::NotFound(format!("repo for server {server_id} not in cache")))?
            .full_name
            .clone()
    };
    let (owner, name) = mapping::split_full_name(&full_name);
    Ok((owner, name))
}

/// Extract `(owner, repo)` from a forum channel ID.
///
/// Handles `fj-issues-{owner}/{repo}`, `fj-pulls-{owner}/{repo}`, and
/// `fj-discussions-{owner}/{repo}`. The `/` separator is unambiguous
/// even when owner or repo names contain hyphens.
pub fn parse_forum_channel(channel_id: &str) -> ClientResult<(String, String)> {
    let rest = channel_id
        .strip_prefix("fj-issues-")
        .or_else(|| channel_id.strip_prefix("fj-pulls-"))
        .or_else(|| channel_id.strip_prefix("fj-discussions-"))
        .ok_or_else(|| ClientError::NotFound(format!("not a forum channel: {channel_id}")))?;
    split_owner_repo(rest)
}
