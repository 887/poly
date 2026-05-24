use poly_client::{ClientError, ClientResult};
use poly_common_forge::split_owner_repo;

/// Extract `(owner, repo)` from a forum channel ID.
///
/// Handles `gh-issues-{owner}/{repo}`, `gh-pulls-{owner}/{repo}`,
/// and `gh-discussions-{owner}/{repo}`.
pub(crate) fn parse_forum_channel(channel_id: &str) -> ClientResult<(String, String)> {
    let rest = channel_id
        .strip_prefix("gh-issues-")
        .or_else(|| channel_id.strip_prefix("gh-pulls-"))
        .or_else(|| channel_id.strip_prefix("gh-discussions-"))
        .ok_or_else(|| ClientError::NotFound(format!("not a forum channel: {channel_id}")))?;
    split_owner_repo(rest)
}
