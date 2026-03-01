//! # poly-devtools-protocol
//!
//! Shared MCP (Model Context Protocol) types and backend trait for Poly devtools.
//!
//! Both desktop-devtools (HTTP eval-bridge) and web-devtools (Chrome CDP) implement
//! the [`DevtoolsBackend`] trait. The MCP JSON-RPC main loop in [`mcp`] dispatches
//! tool calls to whichever backend is active.

pub mod backend;
pub mod mcp;

pub use backend::DevtoolsBackend;
