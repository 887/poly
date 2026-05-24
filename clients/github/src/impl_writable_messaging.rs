//! `impl WritableMessagingBackend for GitHubClient` — per-channel-kind
//! routing for outbound message sends.
//!
//! Split out of [`impl IsBackend`](impl_is_backend.rs) in
//! `plan-trait-split-readable-vs-writable.md` Phase D.8.  GitHub's
//! "writable surface" is highly channel-kind-specific:
//!
//! * `gh-issue-{owner}~{repo}-{number}` — supported, posts an issue
//!   comment via `POST /repos/.../issues/.../comments`.
//! * `gh-issues-*`, `gh-pulls-*`, `gh-discussions-*` — return a
//!   per-kind `NotSupported` error explaining that creating a new
//!   issue/PR/Discussion requires a form-driven workflow.
//! * `gh-code-*` — read-only.
//!
//! The trait method continues to return per-channel `NotSupported`
//! errors for unsupported channel kinds rather than panic (Liskov:
//! same contract as before the split).

use async_trait::async_trait;
use poly_client::{
    ClientError, ClientResult, Message, MessageContent, WritableMessagingBackend,
};
use poly_common_forge::split_owner_repo;

use crate::mapping;
use crate::GitHubClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl WritableMessagingBackend for GitHubClient {
    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        // Route by channel kind — each prefix has a different write semantics.
        if channel_id.starts_with("gh-issues-") {
            return Err(ClientError::NotSupported(
                "GitHub: cannot post to the issues forum index — \
                 use the GitHub web UI to open a new issue"
                    .to_string(),
            ));
        }
        if channel_id.starts_with("gh-pulls-") {
            return Err(ClientError::NotSupported(
                "GitHub: cannot post to the pull-requests forum index — \
                 use the GitHub web UI or CLI to open a pull request"
                    .to_string(),
            ));
        }
        if channel_id.starts_with("gh-discussions-") {
            return Err(ClientError::NotSupported(
                "GitHub: cannot post to the discussions forum index — \
                 use the GitHub web UI to start a new discussion"
                    .to_string(),
            ));
        }
        if channel_id.starts_with("gh-code-") {
            return Err(ClientError::NotSupported(
                "GitHub: code explorer channel is read-only".to_string(),
            ));
        }
        // Single issue/PR thread: gh-issue-{owner}~{repo}-{number}
        if let Some(rest) = channel_id.strip_prefix("gh-issue-") {
            let parts: Vec<&str> = rest.rsplitn(2, '-').collect();
            if let [number_str, rest_pair] = parts.as_slice()
                && let Ok(number) = number_str.parse::<u64>()
            {
                let (owner, repo) = split_owner_repo(rest_pair)?;
                let text = match &content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::WithAttachments { text, .. } => text.clone(),
                };
                let comment = self
                    .cli
                    .create_issue_comment(&owner, &repo, number, &text)
                    .await
                    .map_err(Self::convert_err)?;
                return Ok(mapping::comment_to_message(&comment));
            }
        }
        Err(ClientError::NotSupported(format!(
            "GitHub: unrecognised channel '{channel_id}' — cannot send message"
        )))
    }
}
