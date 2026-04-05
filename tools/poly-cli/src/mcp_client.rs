//! HTTP JSON-RPC client for talking to a poly-chat-mcp server.

use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

/// JSON-RPC client that talks to the MCP server over HTTP.
pub struct McpClient {
    http: reqwest::Client,
    base_url: String,
    next_id: AtomicU64,
}

impl McpClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            next_id: AtomicU64::new(1),
        }
    }

    /// Send a JSON-RPC request and return the result.
    async fn rpc(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let resp = self
            .http
            .post(&self.base_url)
            .json(&req)
            .send()
            .await
            .context("failed to connect to MCP server")?;

        let body: Value = resp.json().await.context("invalid JSON response from MCP")?;

        if let Some(error) = body.get("error") {
            anyhow::bail!(
                "MCP error: {}",
                error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown")
            );
        }

        Ok(body.get("result").cloned().unwrap_or(json!(null)))
    }

    /// Initialize the MCP connection.
    pub async fn initialize(&self) -> Result<Value> {
        self.rpc("initialize", json!({})).await
    }

    /// List all available tools.
    pub async fn list_tools(&self) -> Result<Vec<Value>> {
        let result = self.rpc("tools/list", json!({})).await?;
        let tools = result
            .get("tools")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(tools)
    }

    /// Call a tool with the given arguments.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let params = json!({
            "name": name,
            "arguments": arguments,
        });
        self.rpc("tools/call", params).await
    }
}
