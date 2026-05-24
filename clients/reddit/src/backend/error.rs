//! `RedditError` → `ClientError` conversion + the `NS_*` `NotSupported`
//! message constants shared by all trait-impl modules.
//!
//! Carved out in SOLID-audit-reddit C.3 (was A.3 in the same file).

use crate::RedditError;
use poly_client::ClientError;

impl From<RedditError> for ClientError {
    fn from(e: RedditError) -> Self {
        match e {
            RedditError::LoggedOut => {
                ClientError::AuthFailed("Session cookie missing or expired".to_string())
            }
            RedditError::Status(401) | RedditError::Status(403) => {
                ClientError::AuthFailed(format!("HTTP {}", e))
            }
            RedditError::Status(404) => ClientError::NotFound(e.to_string()),
            RedditError::Http(s) => ClientError::Network(s),
            RedditError::Parse(p) => ClientError::Internal(p.to_string()),
            RedditError::Status(s) => ClientError::Network(format!("HTTP {s}")),
        }
    }
}

// ─── NotSupported message constants ─────────────────────────────────────────
// 18+ identical string allocations collapsed to named constants (A.3).
// Each `ClientError::NotSupported(NS_FOO.to_string())` call still allocates
// at the call site, but the source string lives here once, not 18 times.

pub(crate) const NS_FRIEND_SYSTEM: &str = "Reddit has no friend system";
pub(crate) const NS_USER_NOTE: &str = "Reddit has no user note system";
pub(crate) const NS_BLOCK: &str = "Reddit: block not supported via this interface";
pub(crate) const NS_UNBLOCK: &str = "Reddit: unblock not supported via this interface";
pub(crate) const NS_IGNORE: &str = "Reddit has no ignore concept";
pub(crate) const NS_PRESENCE: &str = "Reddit has no presence system";
pub(crate) const NS_GROUP_DM: &str = "Reddit has no group DMs";
pub(crate) const NS_OPEN_DM: &str = "open_direct_message_channel: not yet implemented for Reddit";
pub(crate) const NS_SAVED_MSG: &str = "open_saved_messages_channel: Reddit has no saved-messages concept";
pub(crate) const NS_CLOSE_DM: &str = "close_dm_channel: not yet implemented for Reddit";
pub(crate) const NS_CONV_MUTE: &str = "Reddit has no conversation mute API";
pub(crate) const NS_TYPING: &str = "Reddit has no typing indicators";
pub(crate) const NS_SEARCH_MSG: &str = "search_messages: Reddit search not yet implemented";
pub(crate) const NS_PINNED_GET: &str = "get_pinned_messages: not supported by Reddit";
pub(crate) const NS_PINNED_SET: &str = "set_message_pinned: not supported by Reddit";
pub(crate) const NS_CREDS: &str = "Reddit only supports EmailPassword and Token credentials";
