//! Mock old.reddit.com server — entry point.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    poly_test_common::run::<poly_test_reddit::RedditState>().await
}
