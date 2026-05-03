//! Shared data types used across all messenger backends.
//!
//! Organised into topic sub-modules. Every type is re-exported flat so
//! `use poly_client::types::Foo` and `use poly_client::Foo` keep working
//! unchanged after this reorganisation.

pub mod backend;
pub mod auth;
pub mod server;
pub mod file;
pub mod message;
pub mod user;
pub mod notification;
pub mod moderation;
pub mod voice;
pub mod command;

// Re-export EVERYTHING flat so `use poly_client::types::Foo` and
// `use poly_client::Foo` keep working unchanged.
pub use backend::*;
pub use auth::*;
pub use server::*;
pub use file::*;
pub use message::*;
pub use user::*;
pub use notification::*;
pub use moderation::*;
pub use voice::*;
pub use command::*;
