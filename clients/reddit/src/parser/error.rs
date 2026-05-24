//! `ParseError` — the single error type produced by every parser in this
//! module.
//!
//! Kept separate from `types.rs` so the error variants can be imported on
//! their own without pulling in the (larger) raw-data structs.

#![cfg(feature = "native")]

use thiserror::Error;

/// Errors produced by every parser in this module.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// Response is the login page — caller's `reddit_session` cookie is
    /// missing or expired.
    #[error("response is a login redirect (cookie missing or expired)")]
    LoggedOut,
    /// A required CSS selector matched zero elements where at least one
    /// was expected (e.g. the comments page contained no OP container).
    #[error("missing required element: {0}")]
    MissingElement(&'static str),
    /// A `data-*` attribute that was expected to be a non-negative
    /// integer (score, timestamp, comment-count) failed to parse.
    #[error("malformed integer attribute: {0}")]
    MalformedInt(&'static str),
    /// A `<time datetime="...">` value failed RFC-3339 parsing.
    #[error("malformed timestamp attribute: {0}")]
    MalformedTimestamp(String),
}
