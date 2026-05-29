//! # Teams Calling — ACS / Microsoft Graph scaffolding
//!
//! Phase A + B of [`docs/plans/plan-teams-calling.md`].
//!
//! Microsoft does not publish a Rust SDK for Azure Communication Services
//! (ACS) Calling. The only realistic delivery path is a JS bridge via a
//! hidden WebView pointed at `@azure/communication-calling` (Phase C of
//! the plan). This module ships **scaffolding only**:
//!
//! - [`types`] — `CallId`, `CallState`, `AcsAccessToken`, `AcsIdentity`,
//!   `CallingError`. Stable wire types that survive the eventual JS-SDK
//!   integration without forcing call-site churn.
//! - [`token`] — token-acquisition path against the ACS Identity REST API
//!   (`POST {acsEndpoint}/identities/{id}/access-tokens`). Compiled and
//!   testable today; needs a real ACS endpoint + bearer to issue tokens
//!   against a live tenant.
//! - [`TeamsCallingClient`] — trait surface the rest of the codebase
//!   consumes. Default impl is a [`StubCallingClient`] that returns
//!   `CallingError::NotSupported`. JS-bridge impl lands separately.
//!
//! The existing [`crate::voice::TeamsVoiceClient`] stays the call-site
//! shim — voice methods still return `NotSupported` — but it now
//! delegates the type contract here so when the JS bridge ships, voice.rs
//! is a one-line wiring change rather than a full rewrite.
//!
//! ## Why the trait split (Interface Segregation)
//!
//! Token acquisition, lifecycle control, and participant queries are
//! three orthogonal capabilities. A future minimal "outbound only" impl
//! shouldn't have to stub participant queries. Each capability lives on
//! its own trait so backends only implement what they support.

// lib.rs already gates this module with #[cfg(feature = "native")]

pub mod types;
pub mod token;
pub mod client;
pub mod ipc;

pub use client::{StubCallingClient, TeamsCallingClient, WebViewBridgeCallingClient};
pub use ipc::{CallingCommand, CallingEvent, CallingTransport, MockCallingTransport};
pub use token::{AcsTokenAcquirer, TokenAcquisitionConfig};
pub use types::{AcsAccessToken, AcsIdentity, CallId, CallState, CallingError};
