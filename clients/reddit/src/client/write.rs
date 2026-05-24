//! Write-side endpoints for [`super::RedditClient`].
//!
//! Carved out in SOLID-audit-reddit B.5. See the parent module for the
//! struct and shared transport helpers.

use super::RedditClient;
use crate::RedditError;

impl RedditClient {
    /// Send a private message (DM).
    ///
    /// **Note:** real `old.reddit.com` `/api/compose` returns 404 as of
    /// 2026-05-02. This works against `poly-test-reddit` (which
    /// implements the legacy form-POST endpoint); against live Reddit a
    /// future update will need to switch to the shreddit GraphQL or
    /// OAuth-bearer path. See plan-reddit-stub.md F.2 findings.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx, `RedditError::LoggedOut`
    /// for a wrong-recipient or unauthenticated reply.
    pub async fn compose_dm(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), RedditError> {
        let url = self.resolve_url("/api/compose");
        let resp = self
            .post_form(
                &url,
                &[
                    ("to", to),
                    ("subject", subject),
                    ("text", body),
                    ("api_type", "json"),
                ],
            )
            .await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        let body: serde_json::Value = resp.json().await?;
        if let Some(errs) = body
            .get("json")
            .and_then(|j| j.get("errors"))
            .and_then(|e| e.as_array())
            && !errs.is_empty()
        {
            return Err(RedditError::LoggedOut);
        }
        Ok(())
    }

    /// Subscribe (or unsubscribe) to a subreddit. `sr_fullname` is the
    /// `t5_<id>` form. `action` is `"sub"` or `"unsub"`.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx.
    pub async fn subscribe(
        &self,
        sr_fullname: &str,
        action: &str,
    ) -> Result<(), RedditError> {
        let url = self.resolve_url("/api/subscribe");
        let resp = self
            .post_form(
                &url,
                &[
                    ("action", action),
                    ("sr", sr_fullname),
                    ("api_type", "json"),
                ],
            )
            .await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        Ok(())
    }

    /// Submit a top-level self-post to a subreddit. Returns the new
    /// post's `t3_<id>` fullname (with the `t3_` prefix) when the API
    /// surfaces it; otherwise an empty string.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx, `RedditError::LoggedOut`
    /// when the response carries a Reddit API errors array.
    pub async fn submit_self_post(
        &self,
        sub: &str,
        title: &str,
        text: &str,
    ) -> Result<String, RedditError> {
        let url = self.resolve_url("/api/submit");
        let resp = self
            .post_form(
                &url,
                &[
                    ("sr", sub),
                    ("kind", "self"),
                    ("title", title),
                    ("text", text),
                    ("api_type", "json"),
                ],
            )
            .await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        let body: serde_json::Value = resp.json().await?;
        if let Some(errs) = body
            .get("json")
            .and_then(|j| j.get("errors"))
            .and_then(|e| e.as_array())
            && !errs.is_empty()
        {
            return Err(RedditError::LoggedOut);
        }
        let name = body
            .get("json")
            .and_then(|j| j.get("data"))
            .and_then(|d| d.get("name"))
            .and_then(|n| n.as_str())
            .map(ToString::to_string)
            .unwrap_or_default();
        Ok(name)
    }

    /// Reply with a comment under a parent (`t1_` for comment-on-comment,
    /// `t3_` for comment-on-post).
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx.
    pub async fn reply_comment(
        &self,
        parent_fullname: &str,
        text: &str,
    ) -> Result<(), RedditError> {
        let url = self.resolve_url("/api/comment");
        let resp = self
            .post_form(
                &url,
                &[
                    ("thing_id", parent_fullname),
                    ("text", text),
                    ("api_type", "json"),
                ],
            )
            .await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        Ok(())
    }

    /// Vote on a thing. `dir` is `1` (upvote), `0` (clear), or `-1`
    /// (downvote). `id` is the `t3_` or `t1_` fullname.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx.
    pub async fn vote(&self, fullname: &str, dir: i8) -> Result<(), RedditError> {
        let url = self.resolve_url("/api/vote");
        let dir_str = dir.to_string();
        let resp = self
            .post_form(&url, &[("id", fullname), ("dir", dir_str.as_str())])
            .await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        Ok(())
    }

    /// Delete an own thing (`t1_<id>` comment or `t3_<id>` post).
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx.
    pub async fn delete_thing(&self, fullname: &str) -> Result<(), RedditError> {
        let url = self.resolve_url("/api/del");
        let resp = self.post_form(&url, &[("id", fullname)]).await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        Ok(())
    }

    /// Edit the body text of an own comment or self-post.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx, `RedditError::LoggedOut`
    /// when Reddit's `errors` array is non-empty.
    pub async fn edit_user_text(
        &self,
        fullname: &str,
        new_text: &str,
    ) -> Result<(), RedditError> {
        let url = self.resolve_url("/api/editusertext");
        let resp = self
            .post_form(
                &url,
                &[
                    ("thing_id", fullname),
                    ("text", new_text),
                    ("api_type", "json"),
                ],
            )
            .await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        let body: serde_json::Value = resp.json().await?;
        if let Some(errs) = body
            .get("json")
            .and_then(|j| j.get("errors"))
            .and_then(|e| e.as_array())
            && !errs.is_empty()
        {
            return Err(RedditError::LoggedOut);
        }
        Ok(())
    }

    /// Mark a DM (`t4_<id>`) or inbox item read.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx.
    pub async fn mark_message_read(&self, fullname: &str) -> Result<(), RedditError> {
        let url = self.resolve_url("/api/read_message");
        let resp = self.post_form(&url, &[("id", fullname)]).await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        Ok(())
    }
}
