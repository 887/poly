//! Test runner that spawns all 5 mock servers simultaneously.
//!
//! Usage: `poly-test-runner [--seed] [--verbose]`
//! Assigns ports 9100-9104 (Matrix, Stoat, Discord, Teams, Poly).

use poly_test_common::CliArgs;

const BACKENDS: &[(&str, u16)] = &[
    ("poly-test-matrix", 9100),
    ("poly-test-stoat", 9101),
    ("poly-test-discord", 9102),
    ("poly-test-teams", 9103),
    ("poly-test-poly", 9104),
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    tracing::info!("poly-test-runner starting all test servers...");

    // TODO(4.9): Spawn each server binary as a child process
    // - Pass --port and optionally --seed/--verbose flags
    // - Wait for /health on each
    // - Print summary table
    // - Handle Ctrl+C: kill all children gracefully

    println!();
    println!("┌─────────────┬───────┬──────────┬──────────────┐");
    println!("│ Backend     │ Port  │ Status   │ Test Animals │");
    println!("├─────────────┼───────┼──────────┼──────────────┤");
    println!("│ Matrix      │ 9100  │ planned  │ Owl, Axolotl │");
    println!("│ Stoat       │ 9101  │ planned  │ Stoat, Raccn │");
    println!("│ Discord     │ 9102  │ planned  │ Koala, Kanga │");
    println!("│ Teams       │ 9103  │ planned  │ Sheep, Walrs │");
    println!("│ Poly        │ 9104  │ planned  │ Cockt, Parrt │");
    println!("└─────────────┴───────┴──────────┴──────────────┘");
    println!();

    for (name, port) in BACKENDS {
        tracing::info!("{name} would start on port {port}");
    }

    tracing::info!("test runner stub complete — implement child process spawning in 4.9");

    Ok(())
}
