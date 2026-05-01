//! Event broadcast infrastructure for real-time event delivery.
//!
//! All chat backends need to push events to connected clients:
//! - Matrix: long-poll `/sync` wakes up when new events arrive
//! - Stoat: Bonfire WebSocket pushes events to connected clients
//! - Discord: Gateway WebSocket dispatches events (MESSAGE_CREATE, etc.)
//! - Teams: change notification callbacks / polling
//! - Poly: WebSocket (inherited from real server)
//!
//! Each backend defines its own event type (e.g. `MatrixSyncEvent`,
//! `StoatBonfireEvent`) and wraps an `EventBus<T>` in its state.
//! REST handlers call `bus.publish(event)` and WS/sync handlers
//! call `bus.subscribe()` to get a receiver.

use tokio::sync::broadcast;

/// Default channel capacity — how many events can buffer before
/// slow receivers start losing old events.
const DEFAULT_CAPACITY: usize = 256;

/// Generic event bus wrapping a `tokio::sync::broadcast` channel.
///
/// `T` is the backend-specific event enum (must be Clone + Send + 'static).
///
/// # Usage
///
/// ```ignore
/// // In state setup:
/// let bus = EventBus::<MyEvent>::new();
///
/// // In REST handler (e.g. send_message):
/// bus.publish(MyEvent::MessageCreate { ... });
///
/// // In WS handler or /sync long-poll:
/// let mut rx = bus.subscribe();
/// loop {
///     match rx.recv().await {
///         Ok(event) => send_to_client(event),
///         Err(broadcast::error::RecvError::Lagged(n)) => {
///             tracing::warn!("client lagged by {n} events");
///         }
///         Err(broadcast::error::RecvError::Closed) => break,
///     }
/// }
/// ```
#[derive(Debug)]
pub struct EventBus<T: Clone + Send + 'static> {
    tx: broadcast::Sender<T>,
}

impl<T: Clone + Send + 'static> EventBus<T> {
    /// Create a new event bus with the default buffer capacity (256).
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a new event bus with a custom buffer capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Publish an event to all subscribers. Returns the number of receivers
    /// that will receive this event. Returns 0 if no one is listening (which
    /// is fine — events before any client connects are just dropped).
    pub fn publish(&self, event: T) -> usize {
        self.tx.send(event).unwrap_or(0)
    }

    /// Subscribe to events. Returns a receiver that will get all events
    /// published after this call.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.tx.subscribe()
    }

    /// Get the current number of active subscribers.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl<T: Clone + Send + 'static> Default for EventBus<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Send + 'static> Clone for EventBus<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}
