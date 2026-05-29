//! Read-side endpoints for [`super::RedditClient`].
//!
//! Carved out in SOLID-audit-reddit B.5. See the parent module for the
//! struct and shared transport helpers.

use super::{urlencoding_simple, RedditClient};
use crate::{parser, RedditError, SortKind, SubredditInfo};
use crate::parser::{RawPost, UserProfile};

impl RedditClient {
    /// List the posts in a subreddit at the given sort.
    ///
    /// Anonymous — works without auth.
    ///
    /// # Errors
    ///
    /// - `RedditError::Status` for HTTP non-2xx.
    /// - `RedditError::Parse` if the HTML structure is unexpected.
    /// - `RedditError::LoggedOut` if Reddit redirects to the login page
    ///   (can happen for quarantined or NSFW-gated subs).
    pub async fn list_subreddit(
        &self,
        subreddit: &str,
        sort: SortKind,
    ) -> Result<Vec<RawPost>, RedditError> {
        let path = format!("/r/{subreddit}/{}/", sort.as_path());
        let base = self.resolve_url(&path);
        let url = match sort.time_filter() {
            Some(t) => format!("{base}?t={t}"),
            None => base,
        };
        let html = self.fetch_text(&url).await?;
        Ok(parser::subreddit::parse_listing(&html)?)
    }

    /// Fetch a single post + its full nested comment tree by post ID.
    ///
    /// The bare `/comments/<id>/` URL works — Reddit 301-redirects to
    /// add the canonical slug, and `reqwest` follows redirects by
    /// default.
    ///
    /// Anonymous — works without auth.
    ///
    /// # Errors
    ///
    /// Same as [`Self::list_subreddit`].
    pub async fn get_post(
        &self,
        post_id: &str,
    ) -> Result<(RawPost, Vec<crate::parser::RawComment>), RedditError> {
        let path = format!("/comments/{post_id}/");
        let url = self.resolve_url(&path);
        let html = self.fetch_text(&url).await?;
        Ok(parser::post::parse_post_page(&html)?)
    }

    /// Fetch the gallery image URLs for a multi-image post.
    ///
    /// Reddit's gallery layout is delivered via `/comments/<id>/.json`,
    /// not the HTML page — `gallery_data.items` ordered list keyed into
    /// `media_metadata.<media_id>.s.u` (the source URL). HTML-entity
    /// encoded URLs in the JSON are decoded.
    ///
    /// Returns `Ok(Vec::new())` for non-gallery posts; the caller can
    /// check `RawPost.is_gallery` first to avoid the round-trip.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx, `RedditError::Http` for
    /// transport-level failures, `RedditError::Parse` if the JSON shape
    /// is unrecognisable.
    pub async fn get_gallery_urls(&self, post_id: &str) -> Result<Vec<String>, RedditError> {
        let path = format!("/comments/{post_id}/.json");
        let url = self.resolve_url(&path);
        let resp = self.with_session_cookie(self.http.get(&url)).send().await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        let json: serde_json::Value = resp.json().await?;
        Ok(parser::post::parse_gallery_metadata(&json)?)
    }

    /// Fetch a user's overview page.
    ///
    /// Anonymous — works without auth.
    ///
    /// # Errors
    ///
    /// Same as [`Self::list_subreddit`]. `RedditError::Status(404)` if
    /// the user doesn't exist or has been suspended.
    pub async fn get_user(&self, username: &str) -> Result<UserProfile, RedditError> {
        let path = format!("/user/{username}/");
        let url = self.resolve_url(&path);
        let html = self.fetch_text(&url).await?;
        Ok(parser::user::parse_user_overview(&html)?)
    }

    /// Fetch the subscribed subreddit slugs for the authenticated user.
    ///
    /// Uses the JSON endpoint (`/subreddits/mine/subscriber/.json`) instead
    /// of the HTML page because Reddit hides subs from new accounts in
    /// HTML for anti-spam reasons (see plan-reddit-stub.md F.2 findings).
    /// JSON shows them immediately.
    ///
    /// Sends the session cookie (manual `X-Mock-Session` + `Cookie`
    /// headers) — anonymous callers get an empty list, not an error.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` on 5xx; `RedditError::Http` on transport.
    pub async fn list_subscribed_subreddits(&self) -> Result<Vec<SubredditInfo>, RedditError> {
        let url = self.resolve_url("/subreddits/mine/subscriber/.json");
        let resp = self.with_session_cookie(self.http.get(&url)).send().await?;
        if !resp.status().is_success() {
            // Anonymous users get 401/403 — return empty list rather than error.
            return Ok(Vec::new());
        }
        let body: serde_json::Value = resp.json().await?;
        let children = body
            .get("data")
            .and_then(|d| d.get("children"))
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(children
            .iter()
            .filter_map(|c| {
                let data = c.get("data")?;
                let name = data.get("display_name").and_then(|n| n.as_str())?;
                // Reddit emits two icon fields and either may be empty:
                //   `community_icon` is the new-style large round icon
                //   `icon_img` is the legacy uploaded image
                // Prefer community_icon when populated, fall back to icon_img.
                // Reddit also HTML-entity-encodes `&` in the URLs (`&amp;`)
                // — decode so reqwest / browser fetch parses the URL.
                let icon_url = ["community_icon", "icon_img"]
                    .iter()
                    .find_map(|field| {
                        data.get(*field)
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.replace("&amp;", "&"))
                    });
                Some(SubredditInfo {
                    name: name.to_string(),
                    icon_url,
                })
            })
            .collect())
    }

    /// Fetch the authenticated user's DM inbox.
    ///
    /// # Errors
    ///
    /// `RedditError::LoggedOut` if the session cookie is missing or
    /// expired (response will be the login page).
    pub async fn inbox(&self) -> Result<Vec<parser::RawDm>, RedditError> {
        let url = self.resolve_url("/message/inbox/");
        let html = self.fetch_text(&url).await?;
        Ok(parser::inbox::parse_inbox(&html)?)
    }

    /// Fetch one page of subreddit posts plus the `after` cursor for
    /// the next page (parsed from old.reddit's `<span class="next-button">`).
    /// Pass `None` for the first page; pass the returned cursor for the
    /// next.
    ///
    /// # Errors
    ///
    /// Same as [`Self::list_subreddit`].
    pub async fn list_subreddit_page(
        &self,
        subreddit: &str,
        sort: SortKind,
        after: Option<&str>,
    ) -> Result<(Vec<RawPost>, Option<String>), RedditError> {
        let mut path = format!("/r/{subreddit}/{}/", sort.as_path());
        let mut params = Vec::<String>::new();
        if let Some(t) = sort.time_filter() {
            params.push(format!("t={t}"));
        }
        if let Some(cursor) = after {
            params.push(format!("count=25&after={}", urlencoding_simple(cursor)));
        }
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }
        let url = self.resolve_url(&path);
        let html = self.fetch_text(&url).await?;
        let posts = parser::subreddit::parse_listing(&html)?;
        let next_after = parser::subreddit::extract_next_after(&html);
        Ok((posts, next_after))
    }

    /// Search subreddits by keyword.
    ///
    /// Returns a page of matching [`SubredditInfo`] items and an optional
    /// `after` cursor for the next page (Reddit's standard pagination token).
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx, `RedditError::Http` for
    /// transport errors, `RedditError::Json` for unparseable responses.
    pub async fn search_subreddits(
        &self,
        query: &str,
        after: Option<&str>,
    ) -> Result<(Vec<SubredditInfo>, Option<String>), RedditError> {
        // Empty query → reddit's "popular" listing (curated by reddit's
        // algorithm). Real reddit serves this at /subreddits/popular.json;
        // the test mock mirrors the path.
        let path = if query.trim().is_empty() {
            let mut p = "/subreddits/popular.json?limit=25".to_string();
            if let Some(cursor) = after {
                let encoded_after = urlencoding_simple(cursor);
                p.push_str("&after=");
                p.push_str(&encoded_after);
            }
            p
        } else {
            let encoded_q = urlencoding_simple(query);
            let mut p = format!("/subreddits/search.json?q={encoded_q}&limit=25");
            if let Some(cursor) = after {
                let encoded_after = urlencoding_simple(cursor);
                p.push_str("&after=");
                p.push_str(&encoded_after);
            }
            p
        };
        let url = self.resolve_url(&path);
        let resp = self.with_session_cookie(self.http.get(&url)).send().await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        let body: serde_json::Value = resp.json().await?;
        let children = body
            .get("data")
            .and_then(|d| d.get("children"))
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        let next_after = body
            .get("data")
            .and_then(|d| d.get("after"))
            .and_then(|a| a.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let subs = children
            .iter()
            .filter_map(|c| {
                let data = c.get("data")?;
                let name = data.get("display_name").and_then(|n| n.as_str())?;
                let icon_url = ["community_icon", "icon_img"]
                    .iter()
                    .find_map(|field| {
                        data.get(*field)
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.replace("&amp;", "&"))
                    });
                Some(SubredditInfo {
                    name: name.to_string(),
                    icon_url,
                })
            })
            .collect();
        Ok((subs, next_after))
    }
}
