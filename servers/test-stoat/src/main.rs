//! Mock Stoat/Revolt API server — entry point.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    poly_test_common::run::<poly_test_stoat::StoatState>().await
}
