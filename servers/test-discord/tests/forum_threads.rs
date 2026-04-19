//! Integration tests for Phase 6: forum channels + thread seed data and routes.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use poly_test_discord::DiscordState;
use std::sync::Arc;
use tower::ServiceExt;

fn seeded_state() -> Arc<DiscordState> {
    let state = Arc::new(DiscordState::new());
    state.seed();
    state
}

fn make_token(state: &Arc<DiscordState>) -> String {
    state.auth.create_token("1")
}

async fn get_json(
    state: &Arc<DiscordState>,
    path: &str,
) -> (StatusCode, serde_json::Value) {
    let router = poly_test_discord::router(Arc::clone(state));
    let token = make_token(state);
    let req = Request::builder()
        .uri(path)
        .header("authorization", format!("Bot {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

// ---------------------------------------------------------------------------
// Forum channel (500) appears in guild 100 channels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_forum_channel_in_guild_channels() {
    let state = seeded_state();
    let (status, json) = get_json(&state, "/api/v10/guilds/100/channels").await;
    assert_eq!(status, StatusCode::OK);
    let arr = json.as_array().unwrap();
    let forum = arr.iter().find(|c| c["id"] == "500");
    assert!(forum.is_some(), "channel 500 missing from guild 100 channels");
    let forum = forum.unwrap();
    assert_eq!(forum["type"], 15, "channel 500 should be GUILD_FORUM (type 15)");
    assert_eq!(forum["name"], "general-discussion");
    let tags = forum["available_tags"].as_array().unwrap();
    assert_eq!(tags.len(), 3);
    let tag_names: Vec<&str> = tags.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tag_names.contains(&"question"));
    assert!(tag_names.contains(&"show-and-tell"));
    assert!(tag_names.contains(&"announcement"));
}

// ---------------------------------------------------------------------------
// GET /api/v10/channels/500 returns forum fields
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_forum_channel_direct() {
    let state = seeded_state();
    let (status, json) = get_json(&state, "/api/v10/channels/500").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["type"], 15);
    assert_eq!(json["default_forum_layout"], 1);
    let tags = json["available_tags"].as_array().unwrap();
    assert_eq!(tags.len(), 3);
}

// ---------------------------------------------------------------------------
// GET /api/v10/guilds/100/threads/active returns active threads
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_guild_active_threads() {
    let state = seeded_state();
    let (status, json) = get_json(&state, "/api/v10/guilds/100/threads/active").await;
    assert_eq!(status, StatusCode::OK);
    let threads = json["threads"].as_array().unwrap();
    // Active threads in guild 100: 501, 502, 511 (503 is archived)
    let ids: Vec<&str> = threads.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"501"), "thread 501 missing from active threads");
    assert!(ids.contains(&"502"), "thread 502 missing from active threads");
    assert!(ids.contains(&"511"), "thread 511 missing from active threads");
    assert!(!ids.contains(&"503"), "archived thread 503 should not be in active threads");
}

// ---------------------------------------------------------------------------
// GET /api/v10/guilds/100/threads/active — threads have correct fields
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_active_thread_fields() {
    let state = seeded_state();
    let (status, json) = get_json(&state, "/api/v10/guilds/100/threads/active").await;
    assert_eq!(status, StatusCode::OK);
    let threads = json["threads"].as_array().unwrap();
    let t501 = threads.iter().find(|t| t["id"] == "501").unwrap();
    assert_eq!(t501["type"], 11, "thread type should be PUBLIC_THREAD (11)");
    assert_eq!(t501["parent_id"], "500");
    assert_eq!(t501["thread_metadata"]["archived"], false);
    assert_eq!(t501["thread_metadata"]["locked"], false);
    let applied = t501["applied_tags"].as_array().unwrap();
    assert!(applied.iter().any(|t| t == "1"), "thread 501 should have tag 1 (question)");
}

// ---------------------------------------------------------------------------
// GET /api/v10/channels/500/threads/archived/public returns archived threads
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_channel_archived_threads() {
    let state = seeded_state();
    let (status, json) = get_json(&state, "/api/v10/channels/500/threads/archived/public").await;
    assert_eq!(status, StatusCode::OK);
    let threads = json["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1, "should be exactly 1 archived thread under channel 500");
    assert_eq!(threads[0]["id"], "503");
    assert_eq!(threads[0]["thread_metadata"]["archived"], true);
    assert_eq!(threads[0]["thread_metadata"]["locked"], true);
}

// ---------------------------------------------------------------------------
// Active threads endpoint returns 404 for unknown guild
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_active_threads_unknown_guild() {
    let state = seeded_state();
    let (status, _) = get_json(&state, "/api/v10/guilds/9999/threads/active").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Archived threads endpoint returns 404 for unknown channel
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_archived_threads_unknown_channel() {
    let state = seeded_state();
    let (status, _) = get_json(&state, "/api/v10/channels/9999/threads/archived/public").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Message in #wildlife-news (510) has inline thread field
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_message_inline_thread_field() {
    let state = seeded_state();
    let (status, json) = get_json(&state, "/api/v10/channels/510/messages").await;
    assert_eq!(status, StatusCode::OK);
    let msgs = json.as_array().unwrap();
    let msg520 = msgs.iter().find(|m| m["id"] == "520").unwrap();
    assert!(msg520.get("thread").is_some(), "msg 520 should have inline thread field");
    assert_eq!(msg520["thread"]["id"], "511");
    assert_eq!(msg520["thread"]["name"], "Koala sighting discussion");
}

// ---------------------------------------------------------------------------
// Media gallery (600) in guild 101 — type 16, Gallery layout
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_media_gallery_channel() {
    let state = seeded_state();
    let (status, json) = get_json(&state, "/api/v10/guilds/101/channels").await;
    assert_eq!(status, StatusCode::OK);
    let arr = json.as_array().unwrap();
    let gallery = arr.iter().find(|c| c["id"] == "600");
    assert!(gallery.is_some(), "channel 600 missing from guild 101 channels");
    let gallery = gallery.unwrap();
    assert_eq!(gallery["type"], 16, "channel 600 should be GUILD_MEDIA (type 16)");
    assert_eq!(gallery["default_forum_layout"], 2);
    let tags = gallery["available_tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
}

// ---------------------------------------------------------------------------
// Media thread (601) OP has an attachment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_media_thread_op_attachment() {
    let state = seeded_state();
    let (status, json) = get_json(&state, "/api/v10/channels/601/messages").await;
    assert_eq!(status, StatusCode::OK);
    let msgs = json.as_array().unwrap();
    let op = msgs.iter().find(|m| m["id"] == "6010").unwrap();
    let attachments = op["attachments"].as_array().unwrap();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0]["filename"], "billabong_sunset.jpg");
    assert_eq!(attachments[0]["content_type"], "image/jpeg");
    assert_eq!(attachments[0]["width"], 1920);
    assert_eq!(attachments[0]["height"], 1080);
}

// ---------------------------------------------------------------------------
// Thread 501 messages (3 messages)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_forum_thread_501_messages() {
    let state = seeded_state();
    let (status, json) = get_json(&state, "/api/v10/channels/501/messages").await;
    assert_eq!(status, StatusCode::OK);
    let msgs = json.as_array().unwrap();
    assert_eq!(msgs.len(), 3);
}

// ---------------------------------------------------------------------------
// Inline thread (511) messages (2 messages)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_inline_thread_511_messages() {
    let state = seeded_state();
    let (status, json) = get_json(&state, "/api/v10/channels/511/messages").await;
    assert_eq!(status, StatusCode::OK);
    let msgs = json.as_array().unwrap();
    assert_eq!(msgs.len(), 2);
}

// ---------------------------------------------------------------------------
// Guild 101 active threads: 601 (media thread, not archived)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_guild_101_active_threads() {
    let state = seeded_state();
    let (status, json) = get_json(&state, "/api/v10/guilds/101/threads/active").await;
    assert_eq!(status, StatusCode::OK);
    let threads = json["threads"].as_array().unwrap();
    let ids: Vec<&str> = threads.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"601"), "thread 601 missing from guild 101 active threads");
}
