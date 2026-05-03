//! Mock old.reddit.com route handlers.
//!
//! Strategy: serve captured fixture HTML with on-the-fly username
//! substitution. The fixtures use `sheep` as the canonical placeholder
//! username (left over from the F.2 capture); the test server replaces
//! `sheep` with the actual logged-in user's name on every response.

#![allow(clippy::module_name_repetitions)]

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::state::{MockDm, RedditState};

#[derive(Debug, Deserialize)]
pub struct EmptyQuery {}

// ── Fixture sources, included at compile time. ──────────────────────────────

const HOT_FIXTURE: &str =
    include_str!("../../../clients/reddit/tests/fixtures/r_rust_hot.html");
const NEW_FIXTURE: &str =
    include_str!("../../../clients/reddit/tests/fixtures/r_rust_new.html");
const TOP_FIXTURE: &str =
    include_str!("../../../clients/reddit/tests/fixtures/r_rust_top.html");
const COMMENTS_FIXTURE: &str =
    include_str!("../../../clients/reddit/tests/fixtures/comments_t3_14921t7.html");
const USER_FIXTURE: &str =
    include_str!("../../../clients/reddit/tests/fixtures/user_overview.html");
const FRONTPAGE_FIXTURE: &str =
    include_str!("../../../clients/reddit/tests/fixtures/frontpage_logged_in.html");
const INBOX_EMPTY_FIXTURE: &str =
    include_str!("../../../clients/reddit/tests/fixtures/inbox_empty.html");
const SUBREDDITS_MINE_EMPTY_FIXTURE: &str =
    include_str!("../../../clients/reddit/tests/fixtures/subreddits_mine_empty.html");
const GALLERY_JSON_FIXTURE: &str =
    include_str!("../../../clients/reddit/tests/fixtures/comments_gallery_t3_1t22ox5.json");
const API_ME_LOGGED_IN_JSON: &str =
    include_str!("../../../clients/reddit/tests/fixtures/api_me_sheep.json");

// ── Helpers ─────────────────────────────────────────────────────────────────

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=')
            && k == name
        {
            return Some(v);
        }
    }
    None
}

fn current_user(state: &RedditState, headers: &HeaderMap) -> Option<String> {
    // Prefer the custom X-Mock-Session header (works in WASM where the
    // browser strips the Cookie header on cross-origin fetch). Fall back
    // to the standard reddit_session cookie for native callers.
    if let Some(value) = headers.get("X-Mock-Session").and_then(|v| v.to_str().ok())
        && let Some(user) = state.user_for_cookie(value)
    {
        return Some(user);
    }
    let cookie = cookie_value(headers, "reddit_session")?;
    state.user_for_cookie(cookie)
}

/// Substitute the fixture's placeholder username (`sheep`) with the
/// requested actual user. Idempotent on already-substituted output.
fn personalise(html: &str, username: &str) -> String {
    if username == "sheep" {
        return html.to_string();
    }
    html.replace("sheep", username)
}

fn html_resp(body: String) -> Response {
    Html(body).into_response()
}

fn json_resp(value: Value) -> Response {
    Json(value).into_response()
}

fn anon_api_me() -> Value {
    // Mirrors the loid-only response that real reddit gives unauth'd.
    json!({
        "loid": "000000000anon",
        "loid_created": 0,
        "data": {
            "modhash": ""
        }
    })
}

// ── GET handlers ────────────────────────────────────────────────────────────

/// `GET /r/<sub>/<sort>/`
pub async fn list_subreddit(
    State(_state): State<Arc<RedditState>>,
    Path((sub, sort)): Path<(String, String)>,
) -> Response {
    let fixture = match sort.as_str() {
        "hot" => HOT_FIXTURE,
        "new" => NEW_FIXTURE,
        "top" => TOP_FIXTURE,
        // rising + controversial fall back to hot for now (parser doesn't
        // care about sort-specific markup, only about the .thing entries).
        "rising" | "controversial" => HOT_FIXTURE,
        _ => return (StatusCode::NOT_FOUND, "unknown sort").into_response(),
    };
    // The fixture content uses "rust" as the subreddit; rewrite if a
    // different sub was requested so `data-subreddit` etc match.
    let body = if sub == "rust" {
        fixture.to_string()
    } else {
        fixture
            .replace("data-subreddit=\"rust\"", &format!("data-subreddit=\"{sub}\""))
            .replace("data-subreddit-prefixed=\"r/rust\"", &format!("data-subreddit-prefixed=\"r/{sub}\""))
            .replace("/r/rust/", &format!("/r/{sub}/"))
    };
    html_resp(body)
}

/// `GET /comments/<id>/` — Reddit's bare-id comments URL, with the
/// real surface returning a 301 to add the slug. We just serve directly.
pub async fn get_post(
    State(_state): State<Arc<RedditState>>,
    Path(post_id): Path<String>,
) -> Response {
    // Real Reddit comment URLs use the bare id (no `t3_` prefix), but
    // some clients accidentally pass the prefixed form — strip it so
    // the fixture rewrite produces a single, valid `t3_<id>` not
    // `t3_t3_<id>`.
    let bare = post_id.strip_prefix("t3_").unwrap_or(&post_id);
    // Always return the deep-thread fixture for now. The fixture's t3_ id
    // is `14921t7` — rewrite in the body so the parser sees the requested id.
    let body = COMMENTS_FIXTURE
        .replace("t3_14921t7", &format!("t3_{bare}"))
        .replace("/14921t7/", &format!("/{bare}/"));
    html_resp(body)
}

/// `GET /r/<sub>/comments/<id>/<slug>/` — the canonical slug variant.
pub async fn get_post_with_slug(
    State(state): State<Arc<RedditState>>,
    Path((_sub, post_id, _slug)): Path<(String, String, String)>,
) -> Response {
    get_post(State(state), Path(post_id)).await
}

/// `GET /comments/<id>/.json` — JSON variant. Returns a real-shape
/// gallery response for `1t22ox5` (the fixture id) so RedditClient's
/// gallery-URL extraction has something to chew on. For other ids,
/// returns an empty (non-gallery) shell.
pub async fn get_post_json(
    State(_state): State<Arc<RedditState>>,
    Path(post_id): Path<String>,
) -> Response {
    let post_id: String = match post_id.strip_prefix("t3_") {
        Some(rest) => rest.to_string(),
        None => post_id,
    };
    if post_id == "1t22ox5" {
        match serde_json::from_str::<Value>(GALLERY_JSON_FIXTURE) {
            Ok(v) => return json_resp(v),
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "fixture parse").into_response(),
        }
    }
    json_resp(json!([
        {"data": {"children": [{"data": {"id": post_id, "is_gallery": false}}]}}
    ]))
}

/// `GET /user/<u>/`
pub async fn get_user(
    State(_state): State<Arc<RedditState>>,
    Path(name): Path<String>,
) -> Response {
    let body = personalise(USER_FIXTURE, &name);
    html_resp(body)
}

/// `GET /api/me.json`
pub async fn api_me(
    State(state): State<Arc<RedditState>>,
    headers: HeaderMap,
) -> Response {
    let Some(user) = current_user(&state, &headers) else {
        return json_resp(anon_api_me());
    };
    // Personalise the captured logged-in JSON with the requested user.
    let body = personalise(API_ME_LOGGED_IN_JSON, &user);
    match serde_json::from_str::<Value>(&body) {
        Ok(v) => json_resp(v),
        Err(_) => json_resp(anon_api_me()),
    }
}

/// `GET /message/inbox/`
pub async fn inbox(
    State(state): State<Arc<RedditState>>,
    headers: HeaderMap,
) -> Response {
    let Some(user) = current_user(&state, &headers) else {
        return logged_out_redirect();
    };
    let dms = state
        .inboxes
        .get(&user)
        .map(|r| r.clone())
        .unwrap_or_default();
    if dms.is_empty() {
        return html_resp(personalise(INBOX_EMPTY_FIXTURE, &user));
    }
    html_resp(synthesise_inbox_html(&user, &dms))
}

/// `GET /subreddits/mine/.json` — populated subscriptions JSON.
pub async fn subreddits_mine_json(
    State(state): State<Arc<RedditState>>,
    headers: HeaderMap,
) -> Response {
    let Some(user) = current_user(&state, &headers) else {
        return (StatusCode::UNAUTHORIZED, "logged out").into_response();
    };
    let subs = state
        .subscriptions
        .get(&user)
        .map(|r| r.clone())
        .unwrap_or_default();
    if subs.is_empty() {
        // Serve the captured populated JSON shape but with empty children.
        return json_resp(json!({
            "kind": "Listing",
            "data": {
                "after": null,
                "dist": 0,
                "modhash": "MODHASH_TEST",
                "children": []
            }
        }));
    }
    // Build a children list from the in-memory subs.
    let children: Vec<Value> = subs
        .iter()
        .enumerate()
        .map(|(i, sub)| {
            // Real reddit serves community_icon from styles.redditmedia.com;
            // the mock returns a deterministic colored letter SVG generated
            // by `/sub-icons/<sub>.svg` so the UI sees per-sub variety
            // without hijacking the user-avatar set (cat/dog belong to
            // /u/cat and /u/dog).
            json!({
                "kind": "t5",
                "data": {
                    "name": format!("t5_test{i}"),
                    "display_name": sub,
                    "display_name_prefixed": format!("r/{sub}"),
                    "title": sub,
                    "subscribers": 1234,
                    "user_is_subscriber": true,
                    "community_icon": format!("http://127.0.0.1:9108/sub-icons/{sub}.svg"),
                    "icon_img": format!("http://127.0.0.1:9108/sub-icons/{sub}.svg"),
                }
            })
        })
        .collect();
    let dist = i64::try_from(children.len()).unwrap_or(0);
    json_resp(json!({
        "kind": "Listing",
        "data": {
            "after": null,
            "dist": dist,
            "modhash": "MODHASH_TEST",
            "children": children,
        }
    }))
}

/// `GET /subreddits/mine/` — HTML; we just serve the captured
/// "anti-spam empty" page since real reddit does the same for new accounts.
pub async fn subreddits_mine_html(
    State(state): State<Arc<RedditState>>,
    headers: HeaderMap,
) -> Response {
    let user = current_user(&state, &headers).unwrap_or_else(|| "anon".to_string());
    html_resp(personalise(SUBREDDITS_MINE_EMPTY_FIXTURE, &user))
}

/// `GET /` (logged-in front page)
pub async fn frontpage(
    State(state): State<Arc<RedditState>>,
    headers: HeaderMap,
) -> Response {
    let user = current_user(&state, &headers).unwrap_or_else(|| "sheep".to_string());
    html_resp(personalise(FRONTPAGE_FIXTURE, &user))
}

/// `GET /login` — the modern shreddit React placeholder.
pub async fn login_page() -> Response {
    // Minimal LoggedOut marker that the parser's detect_logged_out picks up.
    let html = r#"<!DOCTYPE html><html class="theme-beta"><head><title>Welcome to Reddit</title></head><body>login</body></html>"#;
    html_resp(html.to_string())
}

/// Avatar serving — wraps `poly_test_common::avatars::serve_animal`.
pub async fn avatar(Path(animal): Path<String>) -> Response {
    poly_test_common::avatars::serve_animal(&animal)
}

// ── Subreddit search ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct SubredditSearchQuery {
    pub q: Option<String>,
    pub limit: Option<usize>,
    pub after: Option<String>,
}

/// `GET /subreddits/search.json` — search subreddits by keyword.
///
/// Returns a standard Reddit Listing JSON with matching subreddits.
/// Matches against the seeded subreddit names. The `after` cursor mirrors
/// Reddit's pagination token; the test server ignores it (tiny fixture set).
pub async fn subreddits_search(
    State(state): State<Arc<RedditState>>,
    Query(q): Query<SubredditSearchQuery>,
) -> Response {
    let keyword = q
        .q
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    let limit = q.limit.unwrap_or(25);

    // Collect all known subreddit names across subscriptions.
    let mut all_subs: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in state.subscriptions.iter() {
        for sub in entry.value().iter() {
            all_subs.insert(sub.clone());
        }
    }
    // Always include the fixture subreddits + a curated "popular" set so
    // empty-query "popular feed" lookups always return something useful.
    for builtin in POPULAR_SUBS {
        all_subs.insert((*builtin).to_string());
    }

    let all_subs_sorted: Vec<String> = {
        let mut v: Vec<String> = all_subs.into_iter().collect();
        v.sort();
        v
    };

    let children: Vec<Value> = all_subs_sorted
        .iter()
        .filter(|sub| keyword.is_empty() || sub.to_lowercase().contains(&keyword))
        .take(limit)
        .enumerate()
        .map(|(i, sub)| build_subreddit_listing_entry(i, sub, "srch", false))
        .collect();

    let dist = i64::try_from(children.len()).unwrap_or(0);
    json_resp(json!({
        "kind": "Listing",
        "data": {
            "after": null,
            "dist": dist,
            "modhash": "MODHASH_TEST",
            "children": children,
        }
    }))
}

/// Static seed list of popular subreddits used by `subreddits_search`
/// (so an empty query is non-empty) and by `subreddits_popular`.
pub const POPULAR_SUBS: &[&str] = &[
    "rust",
    "programming",
    "askreddit",
    "worldnews",
    "science",
    "technology",
    "todayilearned",
    "showerthoughts",
    "explainlikeimfive",
    "movies",
    "gaming",
    "books",
    "music",
    "personalfinance",
    "lifeprotips",
    "linux",
    "rust_gamedev",
    "selfhosted",
    "futurology",
    "dataisbeautiful",
];

fn build_subreddit_listing_entry(idx: usize, sub: &str, kind_prefix: &str, subscriber: bool) -> Value {
    json!({
        "kind": "t5",
        "data": {
            "name": format!("t5_{kind_prefix}{idx}"),
            "display_name": sub,
            "display_name_prefixed": format!("r/{sub}"),
            "title": sub,
            "subscribers": 5000,
            "user_is_subscriber": subscriber,
            "community_icon": format!("http://127.0.0.1:9108/sub-icons/{sub}.svg"),
            "icon_img": format!("http://127.0.0.1:9108/sub-icons/{sub}.svg"),
            "public_description": format!("The {sub} subreddit."),
        }
    })
}

/// `GET /subreddits/popular.json` — Reddit's "popular subreddits" endpoint.
/// Returns a curated `POPULAR_SUBS` list. Real Reddit cursor-paginates via
/// `after`; the mock returns the whole list in one response with `after: null`.
pub async fn subreddits_popular(
    State(_state): State<Arc<RedditState>>,
) -> Response {
    let children: Vec<Value> = POPULAR_SUBS
        .iter()
        .enumerate()
        .map(|(i, sub)| build_subreddit_listing_entry(i, sub, "pop", false))
        .collect();
    let dist = i64::try_from(children.len()).unwrap_or(0);
    json_resp(json!({
        "kind": "Listing",
        "data": {
            "after": null,
            "dist": dist,
            "modhash": "MODHASH_TEST",
            "children": children,
        }
    }))
}

/// `GET /sub-icons/<sub>.svg` — deterministic-pastel letter SVG for the
/// requested subreddit name. Used in place of hijacking the user-avatar
/// set; first letter of the sub name on a hue-rotated background.
pub async fn sub_icon(Path(sub_with_ext): Path<String>) -> Response {
    let sub = sub_with_ext.strip_suffix(".svg").unwrap_or(&sub_with_ext);
    let letter = sub.chars().next().unwrap_or('r').to_ascii_uppercase();
    // Stable hash → hue so r/rust always renders the same color.
    let hue: u32 = sub.bytes().fold(0u32, |a, b| a.wrapping_add(u32::from(b))) % 360;
    let bg = format!("hsl({hue}, 60%, 45%)");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <circle cx="32" cy="32" r="32" fill="{bg}"/>
  <text x="32" y="40" text-anchor="middle" font-family="-apple-system,Segoe UI,Roboto,sans-serif"
        font-size="32" font-weight="600" fill="white">{letter}</text>
</svg>"#
    );
    Response::builder()
        .header(header::CONTENT_TYPE, "image/svg+xml")
        .body(svg.into())
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "svg build").into_response())
}

// ── POST handlers ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub passwd: String,
    #[serde(default)]
    pub user: Option<String>,
}

/// `POST /api/login/<user>` — mock login.
/// Accepts any password starting with "testpass". Sets the
/// `reddit_session` cookie via Set-Cookie.
pub async fn login(
    State(state): State<Arc<RedditState>>,
    Path(user): Path<String>,
    axum::Form(form): axum::Form<LoginForm>,
) -> Response {
    if !state.users.contains_key(&user) {
        return (
            StatusCode::OK,
            Json(json!({ "json": { "errors": [["WRONG_PASSWORD", "Wrong password", "passwd"]] } })),
        )
            .into_response();
    }
    if !form.passwd.starts_with("testpass") {
        return (
            StatusCode::OK,
            Json(json!({ "json": { "errors": [["WRONG_PASSWORD", "Wrong password", "passwd"]] } })),
        )
            .into_response();
    }
    let cookie = state.issue_session(&user);
    let mut resp = Json(json!({
        "json": {
            "errors": [],
            "data": {
                "modhash": "MODHASH_TEST",
                "cookie": cookie,
            }
        }
    }))
    .into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        format!("reddit_session={cookie}; Path=/; HttpOnly")
            .parse()
            .unwrap_or_else(|_| "reddit_session=invalid".parse().expect("static cookie hdr")),
    );
    resp
}

#[derive(Debug, Deserialize)]
pub struct SubscribeForm {
    pub action: String,
    pub sr: String,
    #[serde(default)]
    pub uh: Option<String>,
}

/// `POST /api/subscribe`
pub async fn subscribe(
    State(state): State<Arc<RedditState>>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<SubscribeForm>,
) -> Response {
    let Some(user) = current_user(&state, &headers) else {
        return (StatusCode::UNAUTHORIZED, "logged out").into_response();
    };
    // sr is the t5_ id; in test mode we just store the raw id as-is.
    let mut subs = state.subscriptions.entry(user.clone()).or_default();
    if form.action == "sub" {
        if !subs.contains(&form.sr) {
            subs.push(form.sr);
        }
    } else if form.action == "unsub" {
        subs.retain(|s| s != &form.sr);
    }
    json_resp(json!({}))
}

#[derive(Debug, Deserialize)]
pub struct ComposeForm {
    pub to: String,
    pub subject: String,
    pub text: String,
    #[serde(default)]
    pub uh: Option<String>,
    #[serde(default)]
    pub api_type: Option<String>,
}

/// `POST /api/compose` — DM compose. Persists DM in-memory; recipient
/// sees it on next `/message/inbox/` fetch.
pub async fn compose(
    State(state): State<Arc<RedditState>>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<ComposeForm>,
) -> Response {
    let Some(user) = current_user(&state, &headers) else {
        return (StatusCode::UNAUTHORIZED, "logged out").into_response();
    };
    if !state.users.contains_key(&form.to) {
        return (
            StatusCode::OK,
            Json(json!({ "json": { "errors": [["USER_DOESNT_EXIST", "user doesn't exist", "to"]] } })),
        )
            .into_response();
    }
    let id = state.record_dm(&user, &form.to, &form.subject, &form.text);
    json_resp(json!({
        "json": { "errors": [], "data": { "id": id } }
    }))
}

#[derive(Debug, Deserialize)]
pub struct CommentForm {
    pub thing_id: String,
    pub text: String,
    #[serde(default)]
    pub uh: Option<String>,
    #[serde(default)]
    pub api_type: Option<String>,
}

/// `POST /api/comment`
pub async fn comment(
    State(state): State<Arc<RedditState>>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<CommentForm>,
) -> Response {
    let Some(user) = current_user(&state, &headers) else {
        return (StatusCode::UNAUTHORIZED, "logged out").into_response();
    };
    let id = state.record_comment(&form.thing_id, &user, &form.text);
    json_resp(json!({
        "json": { "errors": [], "data": { "id": id } }
    }))
}

#[derive(Debug, Deserialize)]
pub struct VoteForm {
    pub id: String,
    pub dir: i8,
    #[serde(default)]
    pub uh: Option<String>,
}

/// `POST /api/vote`
pub async fn vote(
    State(state): State<Arc<RedditState>>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<VoteForm>,
) -> Response {
    let Some(user) = current_user(&state, &headers) else {
        return (StatusCode::UNAUTHORIZED, "logged out").into_response();
    };
    state.record_vote(&form.id, &user, form.dir);
    json_resp(json!({}))
}

/// `POST /test/reset` — wipe in-memory state. Test-only convenience.
pub async fn test_reset(State(state): State<Arc<RedditState>>) -> Response {
    state.sessions.clear();
    state.subscriptions.clear();
    state.inboxes.clear();
    state.sent.clear();
    state.votes.clear();
    state.comments.clear();
    json_resp(json!({ "ok": true }))
}

// ── Utilities ───────────────────────────────────────────────────────────────

fn logged_out_redirect() -> Response {
    let html = r#"<!DOCTYPE html><html class="theme-beta"><head><title>Welcome to Reddit</title></head><body>login</body></html>"#;
    html_resp(html.to_string())
}

/// Build an HTML inbox page from a Vec of MockDm.
/// Synthesised structure matches what `parser::inbox::parse_inbox` expects:
/// `<div class="message" data-fullname="t4_<id>" data-author="<from>">`
/// containing `<a class="subject">`, `<div class="md">`, and
/// `<time class="live-timestamp" datetime="...">`.
fn synthesise_inbox_html(user: &str, dms: &[MockDm]) -> String {
    let mut rows = String::new();
    for dm in dms {
        let dt = dm.when.to_rfc3339();
        let row = format!(
            r##"<div class="message" data-fullname="t4_{id}" data-author="{from}"><a class="subject" href="#">{subj}</a><div class="md">{body}</div><time class="live-timestamp" datetime="{dt}">just now</time></div>"##,
            id = dm.id,
            from = dm.from,
            subj = html_escape(&dm.subject),
            body = html_escape(&dm.body),
            dt = dt,
        );
        rows.push_str(&row);
    }
    let _ = user;
    let _ = Utc::now();
    format!(
        r#"<!DOCTYPE html><html><head><title>messages: inbox</title></head><body><div class="content">{rows}</div></body></html>"#
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
