//! Back-and-forth integration test for poly-reddit + poly-test-reddit.
//!
//! Spins up the mock Reddit server in-process, logs in as 🐱 cat and
//! 🐶 dog, and exercises the canonical "hey come to Signal" DM flow:
//!
//! 1. cat logs in
//! 2. cat browses r/rust hot, drills into a comment thread
//! 3. cat composes a DM to dog ("hey come to Signal")
//! 4. dog logs in (separate client, separate cookie jar)
//! 5. dog reads inbox → sees the DM
//! 6. dog replies (write-side smoke)
//! 7. cat upvotes a post + subscribes to r/programming (write-side smoke)
//!
//! Run with:
//!
//! ```
//! cargo test -p poly-reddit --test back_and_forth
//! ```

#![cfg(feature = "native")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_reddit::{RedditClient, SortKind};
use std::sync::Arc;
use tokio::net::TcpListener;

struct TestServer {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let base_url = format!("http://127.0.0.1:{port}");

        let state = Arc::new(poly_test_reddit::RedditState::seeded());
        let app = poly_test_reddit::router(state);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    drop(rx.await);
                })
                .await
                .expect("server runs");
        });

        // Brief wait for the server to be ready.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        Self {
            base_url,
            _shutdown: tx,
        }
    }
}

#[tokio::test]
async fn cat_dms_dog_full_flow() {
    let server = TestServer::start().await;

    // ── Step 1: cat logs in ──────────────────────────────────────────
    let cat = RedditClient::with_base_url(server.base_url.clone()).expect("cat client");
    cat.login_with_password("cat", "testpass123")
        .await
        .expect("cat logs in");
    let cat_name = cat.is_logged_in().await.expect("auth probe").expect("logged in");
    assert_eq!(cat_name, "cat", "auth probe returns cat");

    // ── Step 2: cat browses r/rust hot ───────────────────────────────
    let posts = cat
        .list_subreddit("rust", SortKind::Hot)
        .await
        .expect("hot listing");
    assert!(!posts.is_empty(), "r/rust hot should have posts");
    let first_post = &posts[0];
    assert_eq!(first_post.subreddit, "rust");

    // Drill into the first post's comment thread.
    let (op, comments) = cat
        .get_post(&first_post.id)
        .await
        .expect("post + comments");
    assert_eq!(op.subreddit, "rust");
    assert!(!comments.is_empty(), "fixture-backed comments");

    // ── Step 3: cat DMs dog the canonical line ───────────────────────
    cat.compose_dm(
        "dog",
        "Hey come to Signal",
        "Heya — Reddit DMs suck, let's move to Signal/Matrix?",
    )
    .await
    .expect("cat sends dm");

    // ── Step 4: dog logs in (fresh client, fresh cookie jar) ──────────
    let dog = RedditClient::with_base_url(server.base_url.clone()).expect("dog client");
    dog.login_with_password("dog", "testpass123")
        .await
        .expect("dog logs in");

    // ── Step 5: dog reads inbox, sees the DM ─────────────────────────
    let inbox = dog.inbox().await.expect("dog reads inbox");
    assert_eq!(inbox.len(), 1, "exactly one DM in dog's inbox");
    let dm = &inbox[0];
    assert_eq!(dm.author, "cat", "dm sender is cat");
    assert_eq!(dm.subject, "Hey come to Signal");
    assert!(
        dm.body_html.contains("Signal") || dm.body_html.contains("Matrix"),
        "body contains Signal or Matrix mention: {}",
        dm.body_html
    );

    // ── Step 6: dog replies via /api/comment (uses t4_ thing_id) ─────
    dog.reply_comment(&format!("t4_{}", dm.id), "On my way!")
        .await
        .expect("dog replies");

    // ── Step 7: cat upvotes the first post + subscribes to a sub ─────
    cat.vote(&format!("t3_{}", first_post.id), 1)
        .await
        .expect("cat upvotes");
    cat.subscribe("t5_2qh16", "sub")
        .await
        .expect("cat subscribes (some sub)");
}

#[tokio::test]
async fn submit_self_post_round_trips() {
    let server = TestServer::start().await;
    let cat = RedditClient::with_base_url(server.base_url.clone()).expect("cat client");
    cat.login_with_password("cat", "testpass123").await.expect("login");

    let name = cat
        .submit_self_post("rust", "Hello /r/rust", "First-time poster, long-time lurker.")
        .await
        .expect("submit succeeds");
    assert!(name.starts_with("t3_"), "name returned with t3_ prefix: {name}");

    // Anonymous client: same call is rejected because no session cookie.
    let anon = RedditClient::with_base_url(server.base_url.clone()).expect("anon client");
    let err = anon
        .submit_self_post("rust", "should fail", "")
        .await
        .expect_err("anon submit rejected");
    assert!(
        matches!(err, poly_reddit::RedditError::Status(401)),
        "expected 401, got {err:?}"
    );
}

#[tokio::test]
async fn anonymous_browse_works_without_login() {
    let server = TestServer::start().await;
    let client = RedditClient::with_base_url(server.base_url.clone()).expect("client");

    let posts = client
        .list_subreddit("rust", SortKind::Hot)
        .await
        .expect("anonymous listing");
    assert!(!posts.is_empty());

    // Trying to read inbox without auth should surface as LoggedOut
    // (test server returns the shreddit React-app marker page).
    let err = client.inbox().await.unwrap_err();
    assert!(matches!(err, poly_reddit::RedditError::Parse(_) | poly_reddit::RedditError::LoggedOut));
}

#[tokio::test]
async fn wrong_password_is_logged_out() {
    let server = TestServer::start().await;
    let client = RedditClient::with_base_url(server.base_url.clone()).expect("client");

    let err = client
        .login_with_password("cat", "wrong-password")
        .await
        .expect_err("wrong password rejected");
    assert!(matches!(err, poly_reddit::RedditError::LoggedOut));
}
