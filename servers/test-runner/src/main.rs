//! Test runner that spawns all mock backend servers simultaneously.
//!
//! Usage: `poly-test-runner [--seed] [--verbose]`
//!
//! Spawns each backend binary as a child process on a fixed port, waits for
//! `/health` to come up, then keeps them alive until Ctrl+C.

use poly_test_common::CliArgs;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::time::sleep;

const BACKENDS: &[(&str, u16)] = &[
    ("poly-test-matrix", 9100),
    ("poly-test-stoat", 9101),
    ("poly-test-discord", 9102),
    ("poly-test-teams", 9103),
    ("poly-test-lemmy", 9104),
    ("poly-test-hackernews", 9105),
    ("poly-test-forgejo", 9106),
    ("poly-test-github", 9107),
];

/// Spawn one backend binary. Returns the child handle.
fn spawn_backend(name: &str, port: u16, args: &CliArgs) -> anyhow::Result<Child> {
    // Run via `cargo run -p <name>` so the binary is rebuilt on demand.
    // This keeps the runner usable from a fresh checkout without a pre-built target.
    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg(name)
        .arg("--")
        .arg("--port")
        .arg(port.to_string());
    if args.seed {
        cmd.arg("--seed");
    }
    if args.verbose {
        cmd.arg("--verbose");
    }
    cmd.kill_on_drop(true);
    let child = cmd.spawn()?;
    Ok(child)
}

/// Poll `/health` until it returns 200 OK or the timeout expires.
async fn wait_for_health(port: u16, timeout: Duration) -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("reqwest client");
    let url = format!("http://127.0.0.1:{}/health", port);
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                return true;
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
    false
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    tracing::info!("poly-test-runner starting all test servers...");

    let mut children: Vec<(String, u16, Child)> = Vec::new();
    for (name, port) in BACKENDS {
        match spawn_backend(name, *port, &args) {
            Ok(child) => {
                tracing::info!("spawned {name} (target port {port})");
                children.push((name.to_string(), *port, child));
            }
            Err(e) => {
                tracing::error!("failed to spawn {name}: {e}");
            }
        }
    }

    // Wait for each backend to become healthy. Cargo may be compiling, so give
    // the first call a generous budget.
    let mut statuses: Vec<(String, u16, bool)> = Vec::new();
    for (name, port, _) in &children {
        tracing::info!("waiting for {name} on port {port}...");
        let healthy = wait_for_health(*port, Duration::from_secs(120)).await;
        if healthy {
            tracing::info!("{name} healthy on port {port}");
        } else {
            tracing::warn!("{name} did NOT become healthy within 120s");
        }
        statuses.push((name.clone(), *port, healthy));
    }

    println!();
    println!("┌──────────────────────┬───────┬──────────┐");
    println!("│ Backend              │ Port  │ Status   │");
    println!("├──────────────────────┼───────┼──────────┤");
    for (name, port, healthy) in &statuses {
        let marker = if *healthy { "up  " } else { "DOWN" };
        println!("│ {:<20} │ {:<5} │ {}     │", name, port, marker);
    }
    println!("└──────────────────────┴───────┴──────────┘");
    println!();
    println!("Test animals available in poly-web:");
    println!("  Matrix   → Owl, Axolotl       (localhost:9100)");
    println!("  Stoat    → Stoat, Raccoon     (localhost:9101)");
    println!("  Discord  → Koala, Kangaroo    (localhost:9102)");
    println!("  Teams    → Sheep, Walrus      (localhost:9103)");
    println!("  Lemmy    → Beaver, Hedgehog   (localhost:9104)");
    println!("  HN       → (read-only feed)   (localhost:9105)");
    println!("  Forgejo  → Otter, Flamingo    (localhost:9106)");
    println!("  GitHub   → Penguin, Chameleon (localhost:9107)");
    println!();
    println!("Press Ctrl+C to stop all servers.");

    // Wait for Ctrl+C, then kill all children.
    tokio::signal::ctrl_c().await?;
    tracing::info!("Ctrl+C received — shutting down all test servers");
    for (name, _port, mut child) in children {
        if let Err(e) = child.kill().await {
            tracing::warn!("failed to kill {name}: {e}");
        }
    }
    Ok(())
}
