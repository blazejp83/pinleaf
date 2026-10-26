//! Pinleaf core domain — pure and portable: no UI, no OS calls. Persistence is abstracted by
//! [`StoragePort`]; v1 ships [`SqliteStore`] (local file) and [`MemoryStore`] (tests/prototyping),
//! and v2 adds a syncing implementation behind the *same* trait. See `docs/ARCHITECTURE.md`.
//!
//! Time is Unix epoch **milliseconds** ([`TimestampMs`]); ids are opaque UUID strings ([`Id`],
//! minted by [`new_id`]).

pub mod id;
pub mod model;
pub mod store;
pub mod store_memory;
pub mod store_sqlite;

pub use id::new_id;
pub use model::*;
pub use store::{Change, ChangeOp, EntityKind, StoragePort, StoreError, StoreResult};
pub use store_memory::MemoryStore;
pub use store_sqlite::SqliteStore;
