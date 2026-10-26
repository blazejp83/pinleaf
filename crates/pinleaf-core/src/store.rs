//! The storage port: the single seam between the domain and persistence.
//!
//! v1 implements this with local SQLite ([`crate::SqliteStore`]); v2 wraps it with a syncing
//! implementation. The append-only [`Change`] feed is used for undo/history in v1 and becomes the
//! sync delta source in v2, so it is part of the port from the start.

use crate::model::{Id, Memoboard, Note, NoteObject, Reminder, TimestampMs};

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Debug)]
pub enum StoreError {
    NotFound,
    Backend(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::NotFound => write!(f, "entity not found"),
            StoreError::Backend(m) => write!(f, "backend error: {m}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// Which entity a change refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    Note,
    Memoboard,
    Reminder,
    Object,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Note => "note",
            EntityKind::Memoboard => "memoboard",
            EntityKind::Reminder => "reminder",
            EntityKind::Object => "object",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "note" => Some(EntityKind::Note),
            "memoboard" => Some(EntityKind::Memoboard),
            "reminder" => Some(EntityKind::Reminder),
            "object" => Some(EntityKind::Object),
            _ => None,
        }
    }
}

/// A create/update vs. a (soft) delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOp {
    Upsert,
    Delete,
}

impl ChangeOp {
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeOp::Upsert => "upsert",
            ChangeOp::Delete => "delete",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "upsert" => Some(ChangeOp::Upsert),
            "delete" => Some(ChangeOp::Delete),
            _ => None,
        }
    }
}

/// One entry in the append-only change feed. `seq` is a monotonic cursor the sync engine advances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub seq: u64,
    pub entity: EntityKind,
    pub id: Id,
    pub op: ChangeOp,
    pub at: TimestampMs,
}

/// Persistence abstraction. Implementations must append a [`Change`] for every mutation and never
/// hard-delete (use the tombstone). Reminders/tags/links join as the PoC grows.
pub trait StoragePort {
    // --- memoboards ---
    fn put_memoboard(&mut self, board: &Memoboard) -> StoreResult<()>;
    fn get_memoboard(&self, id: &str) -> StoreResult<Memoboard>;
    /// Live boards only (tombstoned excluded).
    fn list_memoboards(&self) -> StoreResult<Vec<Memoboard>>;

    // --- notes ---
    fn put_note(&mut self, note: &Note) -> StoreResult<()>;
    fn get_note(&self, id: &str) -> StoreResult<Note>;
    /// Live notes only (tombstoned excluded).
    fn list_notes(&self) -> StoreResult<Vec<Note>>;
    /// Mark a note deleted (tombstone) and log the change; the row is retained for sync.
    fn soft_delete_note(&mut self, id: &str, at: TimestampMs) -> StoreResult<()>;

    // --- reminders (one per note) ---
    fn put_reminder(&mut self, reminder: &Reminder) -> StoreResult<()>;
    fn get_reminder(&self, note_id: &str) -> StoreResult<Option<Reminder>>;
    fn delete_reminder(&mut self, note_id: &str) -> StoreResult<()>;
    /// Reminders due at or before `now` that are still armed (shown, not executed) and whose note
    /// is live (not deleted). Used by the firing scheduler.
    fn due_reminders(&self, now: TimestampMs) -> StoreResult<Vec<Reminder>>;

    // --- note objects (images/attachments; write-once, referenced by id) ---
    fn put_object(&mut self, object: &NoteObject) -> StoreResult<()>;
    /// A live (non-deleted) object by id, bytes included.
    fn get_object(&self, id: &str) -> StoreResult<Option<NoteObject>>;
    fn delete_object(&mut self, id: &str, at: TimestampMs) -> StoreResult<()>;
    /// Live objects belonging to a note.
    fn list_note_objects(&self, note_id: &str) -> StoreResult<Vec<NoteObject>>;

    // --- tags (ordered set per note; set replaces the whole list) ---
    fn set_note_tags(&mut self, note_id: &str, tags: &[String]) -> StoreResult<()>;
    fn get_note_tags(&self, note_id: &str) -> StoreResult<Vec<String>>;

    // --- change feed ---
    /// Changes with `seq` strictly greater than `since`, in order.
    fn changes_since(&self, since: u64) -> StoreResult<Vec<Change>>;
}
