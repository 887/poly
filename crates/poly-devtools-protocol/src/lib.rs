//! # poly-devtools-protocol
//!
//! Shared MCP (Model Context Protocol) types and backend trait for Poly devtools.
//!
//! Both `poly-desktop-devtools-mcp` (HTTP eval-bridge) and `poly-web-devtools-mcp` (Chrome CDP) implement
//! the [`DevtoolsBackend`] trait. The MCP JSON-RPC main loop in [`mcp`] dispatches
//! tool calls to whichever backend is active.

pub mod backend;
pub mod mcp;

pub use backend::DevtoolsBackend;
