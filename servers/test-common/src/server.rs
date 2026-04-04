//! TestServerBase — dynamic port binding, shutdown, and base URL.

use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Shared server base that handles port binding and graceful shutdown.
pub struct TestServerBase {
    pub listener: TcpListener,
    pub addr: SocketAddr,
    pub shutdown_tx: oneshot::Sender<()>,
    pub shutdown_rx: oneshot::Receiver<()>,
}

impl TestServerBase {
    /// Bind to the given port (0 = dynamic/random free port).
    pub async fn bind(port: u16) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(format!("127.0.0.1:{port}")).await?;
        let addr = listener.local_addr()?;
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        Ok(Self {
            listener,
            addr,
            shutdown_tx,
            shutdown_rx,
        })
    }

    /// The base URL for this server (e.g., `http://127.0.0.1:9100`).
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}
