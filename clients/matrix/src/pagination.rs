//! Bridging `MessageQuery`'s message-ID cursors onto Matrix pagination.
//!
//! [`MessageQuery`] addresses history by **message ID** (`before` / `after` /
//! `around`). Matrix `GET /rooms/{id}/messages` paginates with an **opaque
//! token** (`prev_batch` / `start` / `end`) and rejects an event ID passed as
//! `from` with `400 M_INVALID_PARAM`. The two therefore have to be bridged, and
//! this module owns that bridge:
//!
//! * [`MessageCursor`] turns a `MessageQuery` into the single cursor it selects,
//!   or a `NotSupported` naming the conflict — never a silently different page.
//! * [`PaginationTokens`] caches the continuation token of each fetched page
//!   against the event ID of the boundary message it continues from, so the
//!   next call for that message resolves to a real token.
//! * On a cold cache the caller falls back to `/context/{eventId}`, the only
//!   Matrix read endpoint addressed by event ID.

use std::collections::HashMap;
use std::sync::RwLock;

use poly_client::{ClientError, ClientResult, Message, MessageQuery};

use crate::MatrixClient;
use crate::api;

// ─────────────────────────────────────────────────────────────────────────────
// Direction
// ─────────────────────────────────────────────────────────────────────────────

/// Which way a `/messages` request walks the room timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PageDirection {
    /// Towards older history — Matrix `dir=b`.
    Backwards,
    /// Towards newer history — Matrix `dir=f`.
    Forwards,
}

impl PageDirection {
    /// The `dir` query parameter Matrix expects.
    pub(crate) const fn as_matrix_dir(self) -> &'static str {
        match self {
            Self::Backwards => "b",
            Self::Forwards => "f",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Cursor selection
// ─────────────────────────────────────────────────────────────────────────────

/// The one cursor a [`MessageQuery`] selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MessageCursor<'a> {
    /// No cursor — the newest page of the room.
    Newest,
    /// Messages strictly older than this message ID.
    Before(&'a str),
    /// Messages strictly newer than this message ID.
    After(&'a str),
    /// A window centred on this message ID, including it.
    Around(&'a str),
}

/// Refuse a query that supplies more than one cursor.
///
/// Matrix addresses history from a single anchor event, so honouring one cursor
/// and dropping the other would return a page that answers a different question
/// than the caller asked — the exact LSP violation `MessageQuery`'s contract
/// forbids.
fn conflicting_cursors(names: &str) -> ClientError {
    ClientError::NotSupported(format!(
        "Matrix: `get_messages` honours exactly one cursor per call, but {names} were supplied. \
         Matrix pagination walks outwards from a single anchor event; issue one request per cursor."
    ))
}

impl<'a> MessageCursor<'a> {
    /// Classify `query`, or refuse it by name.
    pub(crate) fn from_query(query: &'a MessageQuery) -> ClientResult<Self> {
        match (
            query.before.as_deref(),
            query.after.as_deref(),
            query.around.as_deref(),
        ) {
            (None, None, None) => Ok(Self::Newest),
            (Some(id), None, None) => Ok(Self::Before(id)),
            (None, Some(id), None) => Ok(Self::After(id)),
            (None, None, Some(id)) => Ok(Self::Around(id)),
            (Some(_), Some(_), None) => Err(conflicting_cursors("`before` and `after`")),
            (Some(_), None, Some(_)) => Err(conflicting_cursors("`before` and `around`")),
            (None, Some(_), Some(_)) => Err(conflicting_cursors("`after` and `around`")),
            (Some(_), Some(_), Some(_)) => {
                Err(conflicting_cursors("`before`, `after` and `around`"))
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Token cache
// ─────────────────────────────────────────────────────────────────────────────

/// Upper bound on [`PaginationTokens`] entries.
///
/// Up to two entries per page (one per direction) per room; a long session
/// across many rooms would otherwise grow the map without limit. On overflow it
/// is cleared — the cost is one extra `/context` round trip, not wrong output.
pub(crate) const MAX_PAGINATION_TOKENS: usize = 1024;

/// Whether `id` has the shape of a Matrix event ID rather than a pagination
/// token.
///
/// Event IDs carry the `$` sigil (`$base64hash` on room version 4+,
/// `$random:server.tld` on earlier versions). Pagination tokens never do — they
/// look like `t42-1234_0_0_0` or `s99_1_0`. The distinction matters because
/// sending an event ID as `from` is a `400 M_INVALID_PARAM`, so a cache miss on
/// a `$`-shaped cursor MUST route to `/context` rather than be passed through.
pub(crate) fn looks_like_event_id(id: &str) -> bool {
    id.starts_with('$')
}

/// Continuation tokens for `GET /rooms/{id}/messages`, keyed by the direction
/// they continue in plus the event ID of the boundary message they sit beside.
///
/// Direction is part of the key because the token continuing BACKWARDS from the
/// oldest message of a page and the token continuing FORWARDS from the newest
/// are different opaque strings — and a `/context` window records both, keyed on
/// the same room. Keying on the event ID alone made the second write clobber the
/// first and hand `after` a backwards token.
#[derive(Debug)]
pub(crate) struct PaginationTokens {
    map: RwLock<HashMap<(PageDirection, String), String>>,
}

impl Default for PaginationTokens {
    fn default() -> Self {
        Self::new()
    }
}

impl PaginationTokens {
    /// An empty cache.
    pub(crate) fn new() -> Self {
        Self {
            map: RwLock::new(HashMap::new()),
        }
    }

    /// Remember that `token` continues `dir` from `boundary_event_id`.
    pub(crate) fn record(&self, dir: PageDirection, boundary_event_id: &str, token: &str) {
        if let Ok(mut map) = self.map.write() {
            if map.len() >= MAX_PAGINATION_TOKENS {
                map.clear();
            }
            map.insert((dir, boundary_event_id.to_string()), token.to_string());
        }
    }

    /// Resolve a `MessageQuery` cursor to a Matrix pagination token.
    ///
    /// Returns `None` when the cursor is an event ID with no cached token — the
    /// caller must then go through `/context/{eventId}`. A non-event-shaped
    /// cursor is passed through for callers that already hold a raw
    /// `prev_batch` / `end` token.
    pub(crate) fn resolve(&self, dir: PageDirection, cursor: &str) -> Option<String> {
        let cached = self
            .map
            .read()
            .ok()
            .and_then(|map| map.get(&(dir, cursor.to_string())).cloned());
        match cached {
            Some(token) => Some(token),
            None if looks_like_event_id(cursor) => None,
            None => Some(cursor.to_string()),
        }
    }

    /// Number of cached tokens.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.map.read().map_or(0, |map| map.len())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ordering
// ─────────────────────────────────────────────────────────────────────────────

/// Decode room events that are already in oldest-first order.
///
/// Every `get_messages` consumer assumes oldest-first: the chat view prepends
/// an older page verbatim and chains `after` from `batch.last()`. Matrix serves
/// `/messages?dir=b` and `/context.events_before` **newest-first**, so those
/// arrays must be reversed by the caller before they reach this function.
///
/// Reversal — not a sort on `timestamp` — is the correct transform: Matrix
/// arrays are in the room's topological timeline order, and `origin_server_ts`
/// is the *sending* server's clock, explicitly non-authoritative. Sorting on it
/// would reshuffle a room whose members' clocks disagree, and ties (two events
/// in the same millisecond) would fall back to an arbitrary tiebreak.
pub(crate) fn decode_events<'a>(
    events: impl Iterator<Item = &'a api::RoomEvent>,
) -> Vec<Message> {
    events
        .filter_map(MatrixClient::room_event_to_message)
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Fetch strategies
// ─────────────────────────────────────────────────────────────────────────────

impl MatrixClient {
    /// Token addressing the newest page of `room_id`.
    ///
    /// Prefers the live sync token; falls back to a zero-timeout `/sync` purely
    /// to obtain the room's `prev_batch` when no sync has completed yet.
    async fn newest_page_token(&self, room_id: &str) -> ClientResult<String> {
        let session = self
            .http
            .session()
            .ok_or_else(|| ClientError::AuthFailed("not logged in".into()))?;
        let from = session.sync_next_batch.unwrap_or_default();
        if !from.is_empty() {
            return Ok(from);
        }

        let sync = self.http.sync(None, Some(0)).await?;
        let prev_batch = sync
            .rooms
            .as_ref()
            .and_then(|rooms| rooms.join.as_ref())
            .and_then(|join| join.get(room_id))
            .and_then(|room| room.timeline.as_ref())
            .and_then(|timeline| timeline.prev_batch.clone());
        Ok(prev_batch.unwrap_or(sync.next_batch))
    }

    /// Decode a `/messages` chunk oldest-first and cache its continuation token
    /// against the boundary message that token continues from.
    fn decode_page(&self, response: &api::MessagesResponse, dir: PageDirection) -> Vec<Message> {
        let mut messages = decode_events(response.chunk.iter());
        if dir == PageDirection::Backwards {
            // `dir=b` arrives newest-first.
            messages.reverse();
        }

        let boundary = match dir {
            PageDirection::Backwards => messages.first(),
            PageDirection::Forwards => messages.last(),
        };
        if let (Some(token), Some(boundary)) = (response.end.as_deref(), boundary) {
            self.pagination_tokens.record(dir, &boundary.id, token);
        }
        messages
    }

    /// The newest page of the room — [`MessageCursor::Newest`].
    pub(crate) async fn fetch_newest_page(
        &self,
        room_id: &str,
        limit: u64,
    ) -> ClientResult<Vec<Message>> {
        let dir = PageDirection::Backwards;
        let from = self.newest_page_token(room_id).await?;
        let response = self
            .http
            .fetch_messages(room_id, &from, dir.as_matrix_dir(), Some(limit))
            .await?;
        Ok(self.decode_page(&response, dir))
    }

    /// One page strictly older (`Backwards`) or newer (`Forwards`) than
    /// `cursor` — [`MessageCursor::Before`] / [`MessageCursor::After`].
    pub(crate) async fn fetch_directional_page(
        &self,
        room_id: &str,
        dir: PageDirection,
        cursor: &str,
        limit: u64,
    ) -> ClientResult<Vec<Message>> {
        let Some(from) = self.pagination_tokens.resolve(dir, cursor) else {
            return self.fetch_context_page(room_id, dir, cursor, limit).await;
        };
        let response = self
            .http
            .fetch_messages(room_id, &from, dir.as_matrix_dir(), Some(limit))
            .await?;
        let mut messages = self.decode_page(&response, dir);
        // `before` / `after` are exclusive; a homeserver that echoes the anchor
        // back would otherwise duplicate it into the caller's window.
        messages.retain(|message| message.id != cursor);
        Ok(messages)
    }

    /// Cold-cache path for a directional page: resolve the event ID through
    /// `/context/{eventId}` and serve the side of the window we were asked for.
    async fn fetch_context_page(
        &self,
        room_id: &str,
        dir: PageDirection,
        cursor: &str,
        limit: u64,
    ) -> ClientResult<Vec<Message>> {
        // `/context` splits `limit` evenly between the two sides, so ask for
        // twice what we need to come back with a full page on the side we want.
        let context = self
            .http
            .fetch_context(room_id, cursor, limit.saturating_mul(2))
            .await?;
        let (events, token) = match dir {
            PageDirection::Backwards => (&context.events_before, context.start.as_deref()),
            PageDirection::Forwards => (&context.events_after, context.end.as_deref()),
        };

        let mut messages = decode_events(events.iter());
        if dir == PageDirection::Backwards {
            // `events_before` arrives newest-first.
            messages.reverse();
        }

        let requested = usize::try_from(limit).unwrap_or(usize::MAX);
        let trimmed = messages.len() > requested;
        if trimmed {
            match dir {
                PageDirection::Backwards => {
                    let keep_from = messages.len().saturating_sub(requested);
                    messages = messages.split_off(keep_from);
                }
                PageDirection::Forwards => messages.truncate(requested),
            }
        }

        // `start` / `end` bracket the FULL window `/context` returned. Once the
        // page is trimmed they no longer sit beside the boundary message, so
        // caching them would hand the next call a token that skips history.
        let boundary = match dir {
            PageDirection::Backwards => messages.first(),
            PageDirection::Forwards => messages.last(),
        };
        if !trimmed
            && let (Some(token), Some(boundary)) = (token, boundary)
        {
            self.pagination_tokens.record(dir, &boundary.id, token);
        }
        Ok(messages)
    }

    /// A window centred on `cursor` and including it —
    /// [`MessageCursor::Around`], the jump-to-message primitive.
    pub(crate) async fn fetch_window_around(
        &self,
        room_id: &str,
        cursor: &str,
        limit: u64,
    ) -> ClientResult<Vec<Message>> {
        // The anchor itself counts against `limit`, so ask `/context` for one
        // fewer surrounding event — the window must not exceed what was asked.
        let context = self
            .http
            .fetch_context(room_id, cursor, limit.saturating_sub(1))
            .await?;
        // `events_before` is newest-first; reversing it puts the whole window
        // in the room's timeline order with the anchor in the middle.
        let messages = decode_events(
            context
                .events_before
                .iter()
                .rev()
                .chain(context.event.iter())
                .chain(context.events_after.iter()),
        );

        // Both edges of the window become addressable: `start` continues
        // backwards from the oldest, `end` forwards from the newest. Recording
        // both is what lets the user scroll in either direction after a jump.
        if let (Some(start), Some(oldest)) = (context.start.as_deref(), messages.first()) {
            self.pagination_tokens
                .record(PageDirection::Backwards, &oldest.id, start);
        }
        if let (Some(end), Some(newest)) = (context.end.as_deref(), messages.last()) {
            self.pagination_tokens
                .record(PageDirection::Forwards, &newest.id, end);
        }
        Ok(messages)
    }
}
