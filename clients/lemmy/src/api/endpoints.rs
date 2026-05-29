//! HTTP endpoint methods on `LemmyHttpClient`.
//!
//! Each `pub async fn` corresponds to a single Lemmy REST endpoint and
//! does not contain any pure mapping logic — see `mapping.rs` for the
//! `LemmyType → poly_client::Type` conversion helpers.

use poly_client::{ClientError, ClientResult};
use poly_host_bridge::http::StatusCode;
use serde::Deserialize;

use super::client::LemmyHttpClient;
use super::types::{
    BanFromCommunityRequest, BanFromCommunityResponse, CommentListResponse, CommentView,
    CommunityListResponse, CommunityView, CreateCommentRequest, CreatePostRequest,
    CreatePostResponse, GetModlogResponse, LoginRequest, LoginResponse, ModBanFromCommunityView,
    PostListResponse, PostView, PrivateMessageListResponse, RemoveCommentRequest,
    RemovePostRequest, SearchCommunitiesResponse, SiteResponse,
};

impl LemmyHttpClient {
    /// `POST /api/v3/user/login`
    pub async fn login(&self, username: &str, password: &str) -> ClientResult<LoginResponse> {
        let body = LoginRequest {
            username_or_email: username.to_string(),
            password: password.to_string(),
        };
        let resp = self
            .raw_http()
            .post(self.url("/api/v3/user/login"))
            .header("User-Agent", self.ua())
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::AuthFailed(format!(
                "Login failed: HTTP {}",
                resp.status()
            )));
        }

        resp.json::<LoginResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/site` — fetch current user info.
    pub async fn fetch_site(&self) -> ClientResult<SiteResponse> {
        let jwt = self.jwt()?;
        let resp = self
            .raw_http()
            .get(self.url("/api/v3/site"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/site returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<SiteResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/community/list?type_=Subscribed&limit=50`
    pub async fn fetch_subscribed_communities(&self) -> ClientResult<CommunityListResponse> {
        let jwt = self.jwt()?;
        let resp = self
            .raw_http()
            .get(self.url("/api/v3/community/list?type_=Subscribed&limit=50"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/community/list returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<CommunityListResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/community?id={id}`
    pub async fn fetch_community(&self, community_id: i64) -> ClientResult<CommunityView> {
        // The single-community response wraps in `community_view`
        #[derive(Deserialize)]
        struct SingleCommunityResponse {
            community_view: CommunityView,
        }

        let jwt = self.jwt()?;
        let url = self.url(&format!("/api/v3/community?id={community_id}"));
        let resp = self
            .raw_http()
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if resp.status() == StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound(format!(
                "community {community_id} not found"
            )));
        }
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/community returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<SingleCommunityResponse>()
            .await
            .map(|r| r.community_view)
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/post/list` with explicit sort / page / limit.
    ///
    /// `sort` is passed straight through to Lemmy (`Hot`, `New`, `Top`, …).
    pub async fn fetch_posts_paged(
        &self,
        community_id: i64,
        sort: &str,
        page: u32,
        limit: u32,
    ) -> ClientResult<PostListResponse> {
        let jwt = self.jwt()?;
        // Title-case the sort id so we accept both "hot" and "Hot" from the toolbar.
        let mut chars = sort.chars();
        let sort_param = chars.next().map_or_else(|| "Hot".to_string(), |c| c.to_ascii_uppercase().to_string() + chars.as_str());
        let url = self.url(&format!(
            "/api/v3/post/list?community_id={community_id}&sort={sort_param}&page={page}&limit={limit}"
        ));
        let resp = self
            .raw_http()
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/post/list returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<PostListResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/post?id={id}` — fetch a single post by id.
    pub async fn fetch_post(&self, post_id: i64) -> ClientResult<PostView> {
        #[derive(Deserialize)]
        struct SinglePostResponse {
            post_view: PostView,
        }

        let jwt = self.jwt()?;
        let url = self.url(&format!("/api/v3/post?id={post_id}"));
        let resp = self
            .raw_http()
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if resp.status() == StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound(format!("post {post_id} not found")));
        }
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/post returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<SinglePostResponse>()
            .await
            .map(|r| r.post_view)
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/post/list?community_id={id}&sort=Hot&limit=20`
    pub async fn fetch_posts(&self, community_id: i64) -> ClientResult<PostListResponse> {
        let jwt = self.jwt()?;
        let url = self.url(&format!(
            "/api/v3/post/list?community_id={community_id}&sort=Hot&limit=20"
        ));
        let resp = self
            .raw_http()
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/post/list returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<PostListResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/comment/list?community_id={id}&sort=New&limit={limit}` —
    /// recent comments across ALL posts in a community (Phase D feed toggle).
    pub async fn fetch_community_comments(
        &self,
        community_id: i64,
        limit: u32,
    ) -> ClientResult<CommentListResponse> {
        let jwt = self.jwt()?;
        let url = self.url(&format!(
            "/api/v3/comment/list?community_id={community_id}&sort=New&limit={limit}&type_=All"
        ));
        let resp = self
            .raw_http()
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/comment/list (community) returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<CommentListResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/comment/list?post_id={id}&sort=Hot&limit=50`
    pub async fn fetch_comments(&self, post_id: i64) -> ClientResult<CommentListResponse> {
        let jwt = self.jwt()?;
        let url = self.url(&format!(
            "/api/v3/comment/list?post_id={post_id}&sort=Hot&limit=50"
        ));
        let resp = self
            .raw_http()
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/comment/list returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<CommentListResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/private_message/list?limit=50`
    pub async fn fetch_private_messages(&self) -> ClientResult<PrivateMessageListResponse> {
        let jwt = self.jwt()?;
        let resp = self
            .raw_http()
            .get(self.url("/api/v3/private_message/list?limit=50"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/private_message/list returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<PrivateMessageListResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `POST /api/v3/post` — create a new post in a community (C.7).
    pub async fn create_post(
        &self,
        community_id: i64,
        title: &str,
        body: Option<&str>,
        url: Option<&str>,
    ) -> ClientResult<PostView> {
        let jwt = self.jwt()?;
        let req = CreatePostRequest {
            name: title.to_string(),
            community_id,
            body: body.filter(|s| !s.is_empty()).map(str::to_string),
            url: url.filter(|s| !s.is_empty()).map(str::to_string),
        };

        let resp = self
            .raw_http()
            .post(self.url("/api/v3/post"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&req)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "POST /api/v3/post returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<CreatePostResponse>()
            .await
            .map(|r| r.post_view)
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `POST /api/v3/comment` — create a new comment on a post.
    pub async fn create_comment(
        &self,
        post_id: i64,
        content: &str,
        parent_id: Option<i64>,
    ) -> ClientResult<CommentView> {
        #[derive(Deserialize)]
        struct CommentResponse {
            comment_view: CommentView,
        }

        let jwt = self.jwt()?;
        let body = CreateCommentRequest {
            content: content.to_string(),
            post_id,
            parent_id,
        };

        let resp = self
            .raw_http()
            .post(self.url("/api/v3/comment"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "POST /api/v3/comment returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<CommentResponse>()
            .await
            .map(|r| r.comment_view)
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `POST /api/v3/community/ban_user` — ban or unban a person from a community.
    pub async fn ban_from_community(
        &self,
        req: BanFromCommunityRequest,
    ) -> ClientResult<BanFromCommunityResponse> {
        let jwt = self.jwt()?;
        let resp = self
            .raw_http()
            .post(self.url("/api/v3/community/ban_user"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&req)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if resp.status() == StatusCode::FORBIDDEN {
            return Err(ClientError::PermissionDenied(
                "ban_from_community: permission denied".to_string(),
            ));
        }
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "POST /api/v3/community/ban_user returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<BanFromCommunityResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `POST /api/v3/post/remove` — remove a post as moderator.
    pub async fn remove_post(
        &self,
        post_id: i64,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let jwt = self.jwt()?;
        let body = RemovePostRequest {
            post_id,
            removed: true,
            reason: reason.map(str::to_string),
        };
        let resp = self
            .raw_http()
            .post(self.url("/api/v3/post/remove"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if resp.status() == StatusCode::FORBIDDEN {
            return Err(ClientError::PermissionDenied(
                "remove_post: permission denied".to_string(),
            ));
        }
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "POST /api/v3/post/remove returned HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// `POST /api/v3/comment/remove` — remove a comment as moderator.
    pub async fn remove_comment(
        &self,
        comment_id: i64,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let jwt = self.jwt()?;
        let body = RemoveCommentRequest {
            comment_id,
            removed: true,
            reason: reason.map(str::to_string),
        };
        let resp = self
            .raw_http()
            .post(self.url("/api/v3/comment/remove"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if resp.status() == StatusCode::FORBIDDEN {
            return Err(ClientError::PermissionDenied(
                "remove_comment: permission denied".to_string(),
            ));
        }
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "POST /api/v3/comment/remove returned HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// `GET /api/v3/modlog?community_id={id}&type_=ModBanFromCommunity` — ban history only.
    pub async fn get_modlog_bans(
        &self,
        community_id: i64,
    ) -> ClientResult<Vec<ModBanFromCommunityView>> {
        let jwt = self.jwt()?;
        let url = self.url(&format!(
            "/api/v3/modlog?community_id={community_id}&type_=ModBanFromCommunity"
        ));
        let resp = self
            .raw_http()
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/modlog (bans) returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<GetModlogResponse>()
            .await
            .map(|r| r.banned_from_community)
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/modlog?community_id={id}` — fetch moderation log for a community.
    pub async fn get_modlog(&self, community_id: i64) -> ClientResult<GetModlogResponse> {
        let jwt = self.jwt()?;
        let url = self.url(&format!(
            "/api/v3/modlog?community_id={community_id}&type_=All"
        ));
        let resp = self
            .raw_http()
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/modlog returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<GetModlogResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `PUT /api/v3/community` — update a community (EditCommunity).
    ///
    /// `banner` is a URL string pointing to a previously-uploaded pictrs image
    /// (or any public URL for test purposes). Pass `None` to clear the banner.
    pub async fn put_community(
        &self,
        community_id: i64,
        banner: Option<&str>,
    ) -> ClientResult<CommunityView> {
        #[derive(Deserialize)]
        struct EditCommunityResponse {
            community_view: CommunityView,
        }

        let jwt = self.jwt()?;
        let body = serde_json::json!({
            "community_id": community_id,
            "banner": banner,
            "auth": jwt,
        });
        let resp = self
            .raw_http()
            .put(self.url("/api/v3/community"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "PUT /api/v3/community returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<EditCommunityResponse>()
            .await
            .map(|r| r.community_view)
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/search?type_=Communities&q={query}&listing_type={scope}&limit=50[&page={page}]`
    ///
    /// Maps Lemmy's search endpoint to a `Vec<CommunityView>`. `scope` is one of
    /// `"Subscribed"`, `"Local"`, or `"All"` (Lemmy's `listing_type` values).
    /// `cursor` is an opaque page-number string (`"2"`, `"3"`, …); pass `None`
    /// for the first page.
    pub async fn search_communities(
        &self,
        query: &str,
        listing_type: &str,
        cursor: Option<&str>,
    ) -> ClientResult<SearchCommunitiesResponse> {
        let page = cursor.unwrap_or("1");
        // Empty query → Lemmy's "popular / hot" community feed via the
        // dedicated /community/list endpoint. The response shape is the
        // same `{communities: [...]}` envelope so callers don't branch.
        let url = if query.trim().is_empty() {
            self.url(&format!(
                "/api/v3/community/list?type_={listing_type}&sort=Hot&limit=50&page={page}"
            ))
        } else {
            self.url(&format!(
                "/api/v3/search?type_=Communities&q={query}&listing_type={listing_type}&limit=50&page={page}"
            ))
        };
        let mut req = self.raw_http().get(url).header("User-Agent", self.ua());
        // JWT only when we actually have one — Local / All listings are
        // public on real Lemmy, so an unauthenticated discover view still
        // returns useful results.
        if let Ok(jwt) = self.jwt() {
            req = req.bearer_auth(&jwt);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET community search returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<SearchCommunitiesResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }
}
