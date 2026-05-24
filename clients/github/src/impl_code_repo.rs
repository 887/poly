use async_trait::async_trait;
use poly_client::*;
use poly_common_forge::{decode_b64, kind_from_string};

use crate::mapping;
use crate::{types, GitHubClient};

// ── H.2.a — CodeRepoBackend ──────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::CodeRepoBackend for GitHubClient {
    async fn list_files(&self, channel_id: &str, path: &str) -> ClientResult<Vec<FileEntry>> {
        let (owner, repo) = mapping::parse_code_channel(channel_id)
            .ok_or_else(|| ClientError::NotFound(format!("not a code channel: {channel_id}")))?;
        let contents = self
            .cli
            .get_contents(&owner, &repo, path)
            .await
            .map_err(Self::convert_err)?;
        let entries = match contents {
            types::GhContents::Dir(entries) => entries,
            types::GhContents::File(entry) => vec![entry],
        };
        Ok(entries
            .into_iter()
            .map(|e| FileEntry {
                kind: kind_from_string(&e.kind),
                path: e.path,
                name: e.name,
                size: e.size,
            })
            .collect())
    }

    async fn read_file(&self, channel_id: &str, path: &str) -> ClientResult<FileContent> {
        let (owner, repo) = mapping::parse_code_channel(channel_id)
            .ok_or_else(|| ClientError::NotFound(format!("not a code channel: {channel_id}")))?;
        let contents = self
            .cli
            .get_contents(&owner, &repo, path)
            .await
            .map_err(Self::convert_err)?;
        let entry = match contents {
            types::GhContents::File(e) => e,
            types::GhContents::Dir(_) => {
                return Err(ClientError::NotFound(format!(
                    "{path} is a directory, not a file"
                )));
            }
        };
        let bytes = match (entry.encoding.as_deref(), entry.content) {
            (Some("base64"), Some(b64)) => decode_b64(&b64),
            (_, Some(raw)) => raw.into_bytes(),
            _ => Vec::new(),
        };
        Ok(FileContent {
            path: entry.path,
            bytes,
            truncated: false,
        })
    }
}
