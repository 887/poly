//! Mock Hacker News Firebase API server for Poly testing.
//!
//! Serves a subset of the HN Firebase REST API used by poly-hackernews:
//! - `GET /v0/topstories.json`, etc. — story feed ID lists
//! - `GET /v0/item/:id.json` — story and comment items
//! - `GET /v0/user/:id.json` — user profiles
//! - `GET /health` — health check
//!
//! Default port: 8537.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code
)]

use axum::extract::Path;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use poly_test_common::{health_handler, CliArgs, TestServerBase};
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;

fn make_story(id: u64, title: &str, url: Option<&str>, score: u32, descendants: u32, kids: &[u64]) -> Value {
    let mut story = json!({
        "id": id,
        "type": "story",
        "by": "testuser",
        "time": 1_700_000_000_u64.wrapping_add(id),
        "title": title,
        "score": score,
        "descendants": descendants,
        "kids": kids
    });
    if let Some(u) = url {
        story["url"] = json!(u);
    } else {
        story["text"] = json!(format!("This is the self-text body for story {id}."));
    }
    story
}

fn make_ask_story(id: u64, title: &str, score: u32, descendants: u32, kids: &[u64]) -> Value {
    json!({
        "id": id,
        "type": "story",
        "by": "askuser",
        "time": 1_700_000_000_u64.wrapping_add(id),
        "title": title,
        "text": "What do you all think about this?",
        "score": score,
        "descendants": descendants,
        "kids": kids
    })
}

fn make_comment(id: u64, parent: u64, author: &str, text: &str, kids: &[u64]) -> Value {
    json!({
        "id": id,
        "type": "comment",
        "by": author,
        "time": 1_700_000_100_u64.wrapping_add(id),
        "text": text,
        "parent": parent,
        "kids": kids
    })
}

fn item_for_id(id: u64) -> Option<Value> {
    match id {
        1 => Some(make_story(
            1, "Rust is the most loved language for the 9th year running",
            Some("https://example.com/rust-survey"),
            842, 145,
            &[100, 101, 102],
        )),
        2 => Some(make_story(
            2, "OpenAI releases GPT-5 with 10x context window",
            Some("https://example.com/gpt5"),
            2341, 783,
            &[103, 104],
        )),
        3 => Some(make_story(
            3, "The case for plain text email",
            Some("https://example.com/plain-text"),
            512, 67,
            &[105],
        )),
        4 => Some(make_story(
            4, "Why I switched from Vim to Helix",
            Some("https://example.com/helix-editor"),
            198, 34,
            &[],
        )),
        5 => Some(make_story(
            5, "Postgres 17 released",
            Some("https://www.postgresql.org/about/news/pg17"),
            1124, 201,
            &[106, 107],
        )),
        6 => Some(make_story(
            6, "Show HN: I built a terminal RSS reader in Go",
            Some("https://github.com/testuser/rssterm"),
            87, 22,
            &[],
        )),
        7 => Some(make_story(
            7, "New: Dioxus 0.7 — WASM UI framework for Rust",
            Some("https://dioxuslabs.com/blog/v07"),
            432, 58,
            &[108],
        )),
        8 => Some(make_story(
            8, "The history of the Unix pipe",
            Some("https://example.com/unix-pipe"),
            673, 89,
            &[],
        )),
        9 => Some(make_ask_story(
            9, "Ask HN: What's your favorite CLI tool that nobody knows about?",
            234, 89,
            &[109, 110],
        )),
        10 => Some(make_ask_story(
            10, "Ask HN: How do you manage dotfiles across machines?",
            145, 42,
            &[111],
        )),
        11 => Some(make_story(
            11, "Show HN: I made a keyboard layout optimizer using simulated annealing",
            Some("https://github.com/testuser/keyopt"),
            312, 67,
            &[112],
        )),
        12 => Some(make_story(
            12, "Show HN: Real-time collaborative whiteboard built on CRDTs",
            Some("https://github.com/testuser/crdt-board"),
            89, 15,
            &[],
        )),
        13 => Some(json!({
            "id": 13_u64,
            "type": "job",
            "by": "yc_startup",
            "time": 1_700_000_013_u64,
            "title": "YC W26 startup seeking founding engineer (Rust/Wasm)",
            "url": "https://jobs.example.com/yc-w26",
            "score": 1_u64
        })),
        100 => Some(make_comment(100, 1, "rustlover", "Great language, very fast compile times.", &[113])),
        101 => Some(make_comment(101, 1, "skeptic99", "Until you get to fighting the borrow checker...", &[])),
        102 => Some(make_comment(102, 1, "newbie_coder", "Is it good for beginners?", &[])),
        103 => Some(make_comment(103, 2, "aiwatch", "Context windows keep growing. When does it stop?", &[])),
        104 => Some(make_comment(104, 2, "ml_eng", "Running this in prod. Very impressive for long documents.", &[])),
        105 => Some(make_comment(105, 3, "emailpurist", "HTML email is the bane of my existence.", &[])),
        106 => Some(make_comment(106, 5, "dba_guru", "The new logical replication features are great.", &[])),
        107 => Some(make_comment(107, 5, "pg_fan", "Postgres just keeps getting better every year.", &[])),
        108 => Some(make_comment(108, 7, "wasm_dev", "Dioxus is one of the most ergonomic WASM frameworks I've used.", &[])),
        109 => Some(make_comment(109, 9, "cli_wizard", "eza (modern ls replacement) and bat (modern cat).", &[])),
        110 => Some(make_comment(110, 9, "poweruser42", "fzf, zoxide, and ripgrep are my holy trinity.", &[])),
        111 => Some(make_comment(111, 10, "dotfile_mgr", "I use chezmoi with a private git repo. Works great.", &[])),
        112 => Some(make_comment(112, 11, "keyboard_nerd", "Which corpus did you use for the optimization?", &[])),
        113 => Some(make_comment(113, 100, "rustlover", "Though with incremental compilation it's gotten much better.", &[])),
        _ => None,
    }
}

async fn get_topstories() -> impl IntoResponse {
    Json(json!([1_u64, 2_u64, 3_u64, 4_u64, 5_u64]))
}

async fn get_newstories() -> impl IntoResponse {
    Json(json!([6_u64, 7_u64, 8_u64]))
}

async fn get_beststories() -> impl IntoResponse {
    Json(json!([1_u64, 2_u64]))
}

async fn get_askstories() -> impl IntoResponse {
    Json(json!([9_u64, 10_u64]))
}

async fn get_showstories() -> impl IntoResponse {
    Json(json!([11_u64, 12_u64]))
}

async fn get_jobstories() -> impl IntoResponse {
    Json(json!([13_u64]))
}

async fn get_item(Path(id): Path<u64>) -> impl IntoResponse {
    match item_for_id(id) {
        Some(item) => Json(item).into_response(),
        None => Json(Value::Null).into_response(),
    }
}

async fn get_user(Path(username): Path<String>) -> impl IntoResponse {
    match username.as_str() {
        "testuser" | "rustlover" | "skeptic99" | "aiwatch" | "ml_eng" | "dba_guru" | "pg_fan"
        | "wasm_dev" | "cli_wizard" | "poweruser42" | "dotfile_mgr" | "keyboard_nerd"
        | "emailpurist" | "newbie_coder" => Json(json!({
            "id": username,
            "created": 1_500_000_000_u64,
            "karma": 42_u64,
            "about": format!("HN user {username}")
        }))
        .into_response(),
        "askuser" => Json(json!({
            "id": "askuser",
            "created": 1_400_000_000_u64,
            "karma": 1337_u64,
            "about": "Asks good questions."
        }))
        .into_response(),
        _ => Json(Value::Null).into_response(),
    }
}

async fn get_item_json(Path(id_json): Path<String>) -> impl IntoResponse {
    let id_str = id_json.trim_end_matches(".json");
    let id: u64 = match id_str.parse() {
        Ok(n) => n,
        Err(_) => return Json(Value::Null).into_response(),
    };
    get_item(Path(id)).await.into_response()
}

async fn get_user_json(Path(id_json): Path<String>) -> impl IntoResponse {
    let username = id_json.trim_end_matches(".json").to_string();
    get_user(Path(username)).await.into_response()
}

async fn get_updates() -> impl IntoResponse {
    Json(json!({
        "items": [1_u64, 2_u64],
        "profiles": ["testuser"]
    }))
}

fn router() -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { health_handler("hackernews").await }),
        )
        .route("/v0/topstories.json", get(get_topstories))
        .route("/v0/newstories.json", get(get_newstories))
        .route("/v0/beststories.json", get(get_beststories))
        .route("/v0/askstories.json", get(get_askstories))
        .route("/v0/showstories.json", get(get_showstories))
        .route("/v0/jobstories.json", get(get_jobstories))
        .route("/v0/item/{id_json}", get(get_item_json))
        .route("/v0/user/{id_json}", get(get_user_json))
        .route("/v0/updates.json", get(get_updates))
        .layer(CorsLayer::very_permissive())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-hackernews listening on {}", base.base_url());

    let app = router();
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            drop(base.shutdown_rx.await);
        })
        .await?;

    Ok(())
}
