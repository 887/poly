//! Cross-backend `MessageQuery` cursor-contract test (plan Phase D).
//!
//! `MessageQuery` documents three cursors — `before`, `after`, `around` — in
//! terms of MESSAGE IDs. Every `IsBackend` impl must honour that contract or
//! return `NotSupported`; none may substitute a different page. That is SOLID
//! item 3 (Liskov): swapping `poly-matrix` for `poly-demo` behind the same
//! `&dyn IsBackend` must not change which messages the caller receives.
//!
//! [`assert_message_query_contract`] is written against `&dyn IsBackend` with
//! no Matrix in its signature precisely so it can be run against any backend.
//! It lives here only because `clients/matrix/` is the scope of this change —
//! see the plan's Phase D note for the shared home it belongs in.
//!
//! Run with: `cargo test -p poly-matrix --test message_query_contract`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use axum::extract::{Path, Query};
use axum::routing::get;
use axum::{Json, Router};
use tokio::net::TcpListener;

use poly_client::{AuthCredentials, ClientError, IsBackend, MessageContent, MessageQuery};
use poly_matrix::MatrixClient;
use poly_test_matrix::{MatrixState, router};

/// Room seeded by `MatrixState::seed`, extended by the harness below.
const ROOM_ID: &str = "!general1:localhost";

/// Total messages the harness guarantees in [`ROOM_ID`].
const SEEDED_MESSAGES: usize = 40;

/// Page size used throughout, small enough that 40 messages span several pages.
const PAGE: u32 = 8;

/// [`PAGE`] as a length.
fn page_len() -> usize {
    usize::try_from(PAGE).expect("PAGE fits in usize")
}

// ---------------------------------------------------------------------------
// Mock homeserver + the `/context/{eventId}` route it is missing
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct ContextParams {
    limit: Option<usize>,
}

/// Spec-faithful `GET /rooms/{roomId}/context/{eventId}` over the mock's
/// timeline.
///
/// `poly-test-matrix` does not serve this endpoint (see the blocked finding in
/// the plan), so the test supplies it. Semantics mirror Synapse:
/// `events_before` is reverse-chronological, `events_after` chronological, the
/// homeserver splits `limit` evenly between the two sides, and `start` / `end`
/// are tokens that continue backwards / forwards past the window — expressed in
/// the mock's numeric-index token scheme so `/messages` accepts them.
async fn context_handler(
    state: Arc<MatrixState>,
    Path((room_id, event_id)): Path<(String, String)>,
    Query(params): Query<ContextParams>,
) -> Json<serde_json::Value> {
    let Some(timeline) = state.timelines.get(&room_id).map(|entry| entry.clone()) else {
        return Json(serde_json::json!({}));
    };
    let Some(index) = timeline
        .iter()
        .position(|event| event.get("event_id").and_then(|id| id.as_str()) == Some(&event_id))
    else {
        return Json(serde_json::json!({}));
    };

    let half = params.limit.unwrap_or(10).div_euclid(2);
    let before_start = index.saturating_sub(half);
    let events_before: Vec<serde_json::Value> = timeline
        .get(before_start..index)
        .unwrap_or_default()
        .iter()
        .rev()
        .cloned()
        .collect();
    let after_start = index.saturating_add(1);
    let after_end = after_start.saturating_add(half).min(timeline.len());
    let events_after: Vec<serde_json::Value> = timeline
        .get(after_start..after_end)
        .unwrap_or_default()
        .to_vec();

    Json(serde_json::json!({
        "events_before": events_before,
        "event": timeline.get(index).cloned(),
        "events_after": events_after,
        "start": before_start.to_string(),
        "end": after_end.to_string(),
        "state": [],
    }))
}

struct TestServer {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let base_url = format!("http://{addr}");

        let state = Arc::new(MatrixState::new());
        state.seed();

        let context_state = Arc::clone(&state);
        let app: Router = router(Arc::clone(&state)).route(
            "/_matrix/client/v3/rooms/{roomId}/context/{eventId}",
            get(move |path, query| context_handler(Arc::clone(&context_state), path, query)),
        );

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                })
                .await
                .expect("serve");
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        Self {
            base_url,
            _shutdown: shutdown_tx,
        }
    }
}

async fn test_token(base_url: &str) -> String {
    let resp: serde_json::Value = reqwest::Client::new()
        .post(format!("{base_url}/test/auth/token"))
        .json(&serde_json::json!({ "username": "Owl" }))
        .send()
        .await
        .expect("token request")
        .json()
        .await
        .expect("token json");
    resp.get("access_token")
        .and_then(serde_json::Value::as_str)
        .expect("access_token")
        .to_string()
}

/// An authenticated client whose token cache is COLD — no page fetched yet.
async fn cold_client(base_url: &str) -> MatrixClient {
    let mut client = MatrixClient::with_homeserver(base_url).expect("homeserver url");
    let token = test_token(base_url).await;
    let _session = client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");
    client
}

/// Fetch a page, failing the test with `label` on error.
async fn page(
    backend: &dyn IsBackend,
    channel_id: &str,
    query: MessageQuery,
    label: &str,
) -> Vec<String> {
    backend
        .get_messages(channel_id, query)
        .await
        .unwrap_or_else(|error| panic!("{label} must be honoured or refused, got {error:?}"))
        .into_iter()
        .map(|message| message.id)
        .collect()
}

/// Grow [`ROOM_ID`] to [`SEEDED_MESSAGES`] events and return every message ID
/// in timeline order, oldest first.
async fn seed_and_read_room(client: &MatrixClient) -> Vec<String> {
    let full = MessageQuery {
        limit: Some(500),
        ..Default::default()
    };
    let existing = page(client, ROOM_ID, full.clone(), "initial history").await;

    for i in existing.len()..SEEDED_MESSAGES {
        let _sent =
            IsBackend::send_message(client, ROOM_ID, MessageContent::Text(format!("filler {i}")))
                .await
                .expect("send filler");
    }

    let all = page(client, ROOM_ID, full, "full history").await;
    assert_eq!(
        all.len(),
        SEEDED_MESSAGES,
        "harness must seed exactly {SEEDED_MESSAGES} messages"
    );
    all
}

// ---------------------------------------------------------------------------
// The contract itself — no Matrix in these signatures
// ---------------------------------------------------------------------------

/// Map a page of message IDs onto their positions in the full timeline.
fn positions(timeline: &[String], returned: &[String], label: &str) -> Vec<usize> {
    returned
        .iter()
        .map(|id| {
            timeline
                .iter()
                .position(|other| other == id)
                .unwrap_or_else(|| panic!("{label} returned {id}, which is not in the timeline"))
        })
        .collect()
}

/// Assert a page is oldest-first and within `limit`.
fn assert_page_shape(indices: &[usize], label: &str) {
    assert!(
        indices.len() <= page_len(),
        "{label} exceeded limit: {}",
        indices.len()
    );
    assert!(
        indices.is_sorted_by(|left, right| left < right),
        "{label} is not oldest-first: {indices:?}"
    );
}

/// `before(anchor)`: strictly older, contiguous with the anchor, oldest-first.
async fn assert_before_contract(
    backend: &dyn IsBackend,
    channel_id: &str,
    timeline: &[String],
    anchor_index: usize,
) {
    let anchor = timeline.get(anchor_index).expect("anchor").clone();
    let older = page(
        backend,
        channel_id,
        MessageQuery {
            before: Some(anchor),
            limit: Some(PAGE),
            ..Default::default()
        },
        "before(anchor)",
    )
    .await;
    assert!(!older.is_empty(), "before(anchor) returned nothing");

    let indices = positions(timeline, &older, "before(anchor)");
    assert_page_shape(&indices, "before(anchor)");
    assert!(
        indices.iter().all(|index| *index < anchor_index),
        "before(anchor) returned a message at or after the anchor: {indices:?} vs {anchor_index}"
    );
    assert_eq!(
        indices.last().copied(),
        anchor_index.checked_sub(1),
        "before(anchor) must be the page immediately preceding the anchor"
    );
}

/// `after(anchor)`: strictly newer, contiguous with the anchor, oldest-first.
async fn assert_after_contract(
    backend: &dyn IsBackend,
    channel_id: &str,
    timeline: &[String],
    anchor_index: usize,
) {
    let anchor = timeline.get(anchor_index).expect("anchor").clone();
    let newer = page(
        backend,
        channel_id,
        MessageQuery {
            after: Some(anchor),
            limit: Some(PAGE),
            ..Default::default()
        },
        "after(anchor)",
    )
    .await;
    assert!(!newer.is_empty(), "after(anchor) returned nothing");

    let indices = positions(timeline, &newer, "after(anchor)");
    assert_page_shape(&indices, "after(anchor)");
    assert!(
        indices.iter().all(|index| *index > anchor_index),
        "after(anchor) returned a message at or before the anchor: {indices:?} vs {anchor_index}"
    );
    assert_eq!(
        indices.first().copied(),
        Some(anchor_index.saturating_add(1)),
        "after(anchor) must be the page immediately following the anchor"
    );
}

/// `around(anchor)`: a window that CONTAINS the anchor, with context on both
/// sides — the assertion the jump-to-message bug failed.
async fn assert_around_contract(
    backend: &dyn IsBackend,
    channel_id: &str,
    timeline: &[String],
    anchor_index: usize,
) {
    let anchor = timeline.get(anchor_index).expect("anchor").clone();
    let window = page(
        backend,
        channel_id,
        MessageQuery {
            around: Some(anchor.clone()),
            limit: Some(PAGE),
            ..Default::default()
        },
        "around(anchor)",
    )
    .await;
    assert!(
        window.contains(&anchor),
        "around(anchor) did not include the anchor — this is the jump-to-message bug"
    );

    let indices = positions(timeline, &window, "around(anchor)");
    assert_page_shape(&indices, "around(anchor)");
    assert!(
        indices.first().is_some_and(|first| *first < anchor_index),
        "around(anchor) has no context ABOVE the anchor: {indices:?}"
    );
    assert!(
        indices.last().is_some_and(|last| *last > anchor_index),
        "around(anchor) has no context BELOW the anchor: {indices:?}"
    );
}

/// A query the backend cannot answer must refuse, never guess.
async fn assert_unanswerable_query_refuses(
    backend: &dyn IsBackend,
    channel_id: &str,
    anchor: String,
) {
    let conflict = backend
        .get_messages(
            channel_id,
            MessageQuery {
                before: Some(anchor.clone()),
                after: Some(anchor),
                limit: Some(PAGE),
                ..Default::default()
            },
        )
        .await;
    match conflict {
        Err(ClientError::NotSupported(_)) => {}
        Ok(messages) => panic!(
            "a query the backend cannot answer returned {} messages instead of NotSupported",
            messages.len()
        ),
        Err(other) => panic!("expected NotSupported, got {other:?}"),
    }
}

/// Assert `backend` honours the documented `MessageQuery` cursor contract for
/// `channel_id`, whose full oldest-first message-ID order is `timeline`.
///
/// Every assertion is contract text, not Matrix behaviour: pages come back
/// oldest-first and within `limit`; `before` / `after` are exclusive of the
/// anchor and contiguous with it; `around` includes it; and a cursor the
/// backend cannot honour yields `NotSupported` rather than a different page.
async fn assert_message_query_contract(
    backend: &dyn IsBackend,
    channel_id: &str,
    timeline: &[String],
) {
    let anchor_index = timeline.len().div_euclid(2);
    let anchor = timeline.get(anchor_index).expect("anchor exists").clone();

    assert_before_contract(backend, channel_id, timeline, anchor_index).await;
    assert_after_contract(backend, channel_id, timeline, anchor_index).await;
    assert_around_contract(backend, channel_id, timeline, anchor_index).await;
    assert_unanswerable_query_refuses(backend, channel_id, anchor).await;
}

// ---------------------------------------------------------------------------
// Matrix runs of the contract
// ---------------------------------------------------------------------------

/// Cold token cache: every cursor has to resolve through `/context/{eventId}`.
/// This is the app-restart / permalink case that used to send an event ID as
/// `from` and get `400 M_INVALID_PARAM`.
#[tokio::test]
async fn matrix_honours_the_cursor_contract_on_a_cold_cache() {
    let srv = TestServer::start().await;
    let seeder = cold_client(&srv.base_url).await;
    let timeline = seed_and_read_room(&seeder).await;

    // A brand-new client shares nothing with `seeder` — its token map is empty.
    let client = cold_client(&srv.base_url).await;
    assert_message_query_contract(&client, ROOM_ID, &timeline).await;
}

/// Warm token cache: `before` resolves through the token recorded by the
/// preceding page rather than through `/context`.
#[tokio::test]
async fn matrix_honours_the_cursor_contract_on_a_warm_cache() {
    let srv = TestServer::start().await;
    let client = cold_client(&srv.base_url).await;
    let timeline = seed_and_read_room(&client).await;
    assert_message_query_contract(&client, ROOM_ID, &timeline).await;
}

/// Infinite scroll: three consecutive `before` pages must walk strictly
/// backwards without repeating or skipping. Before the token map existed this
/// looped on the same page forever.
#[tokio::test]
async fn scrolling_up_walks_backwards_page_after_page() {
    let srv = TestServer::start().await;
    let client = cold_client(&srv.base_url).await;
    let timeline = seed_and_read_room(&client).await;

    let first_page = page(
        &client,
        ROOM_ID,
        MessageQuery {
            limit: Some(PAGE),
            ..Default::default()
        },
        "newest page",
    )
    .await;
    assert_eq!(first_page.len(), page_len());

    let mut cursor = first_page.first().expect("non-empty page").clone();
    let mut seen = first_page;

    for step in 0_u32..3 {
        let older = page(
            &client,
            ROOM_ID,
            MessageQuery {
                before: Some(cursor.clone()),
                limit: Some(PAGE),
                ..Default::default()
            },
            "older page",
        )
        .await;
        assert_eq!(older.len(), page_len(), "short page at step {step}");
        for id in &older {
            assert!(
                !seen.contains(id),
                "step {step} repeated {id} — pagination is not advancing"
            );
        }
        cursor = older.first().expect("non-empty older page").clone();
        let _spliced = seen.splice(0..0, older).count();
    }

    // Four pages of eight walked contiguously back from the newest message.
    let expected = timeline
        .get(timeline.len().saturating_sub(seen.len())..)
        .expect("tail slice");
    assert_eq!(seen, expected, "scroll-up did not reconstruct the timeline");
}

/// Jumping to a search hit lands ON the message, with history either side, and
/// scrolling stays coherent afterwards because `around` records BOTH edge
/// tokens.
#[tokio::test]
async fn jumping_to_a_message_lands_on_it_and_can_scroll_both_ways() {
    let srv = TestServer::start().await;
    let client = cold_client(&srv.base_url).await;
    let timeline = seed_and_read_room(&client).await;
    let target = timeline.get(12).expect("target message").clone();

    let window = page(
        &client,
        ROOM_ID,
        MessageQuery {
            around: Some(target.clone()),
            limit: Some(PAGE),
            ..Default::default()
        },
        "around(target)",
    )
    .await;
    assert!(window.contains(&target));

    let oldest = window.first().expect("window non-empty").clone();
    let newest = window.last().expect("window non-empty").clone();
    let index_of = |id: &str| timeline.iter().position(|other| other == id).expect("index");

    let above = page(
        &client,
        ROOM_ID,
        MessageQuery {
            before: Some(oldest.clone()),
            limit: Some(PAGE),
            ..Default::default()
        },
        "scroll up from the jump window",
    )
    .await;
    assert_eq!(
        above.last(),
        timeline.get(index_of(&oldest).saturating_sub(1)),
        "scrolling up from a jump window skipped or repeated history"
    );

    let below = page(
        &client,
        ROOM_ID,
        MessageQuery {
            after: Some(newest.clone()),
            limit: Some(PAGE),
            ..Default::default()
        },
        "scroll down from the jump window",
    )
    .await;
    assert_eq!(
        below.first(),
        timeline.get(index_of(&newest).saturating_add(1)),
        "scrolling down from a jump window skipped or repeated history"
    );
}

/// The cold-cache guard itself: an event ID must never reach `/messages` as
/// `from`. The mock parses `from` as a numeric index and silently falls back to
/// the newest page on a parse failure, which is exactly how the old bug hid —
/// so assert on the PAGE, not on an error code.
#[tokio::test]
async fn a_cold_event_id_cursor_does_not_return_the_newest_page() {
    let srv = TestServer::start().await;
    let seeder = cold_client(&srv.base_url).await;
    let timeline = seed_and_read_room(&seeder).await;
    let anchor = timeline.get(10).expect("anchor").clone();

    let client = cold_client(&srv.base_url).await;
    let older = page(
        &client,
        ROOM_ID,
        MessageQuery {
            before: Some(anchor),
            limit: Some(PAGE),
            ..Default::default()
        },
        "cold before()",
    )
    .await;

    let newest = timeline.last().expect("newest message");
    assert!(
        !older.iter().any(|id| id == newest),
        "a cold `before` cursor fell through to the newest page"
    );
}
