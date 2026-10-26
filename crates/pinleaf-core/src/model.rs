//! Core entities and enums. The integer enum values are stable on-disk codes for the SQLite schema.

/// Unix epoch milliseconds. One time unit across the whole app (converted only at the SQLite edge).
pub type TimestampMs = i64;

/// Stable, client-generated identifier (UUID string). Core treats ids as opaque; generation lives
/// in the application layer (production uses the `uuid` crate).
pub type Id = String;

/// Defines a C-like enum with explicit integer discriminants plus `as_i32` / `from_i32`.
macro_rules! int_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $val:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($variant = $val),+ }

        impl $name {
            /// The stored integer value.
            pub fn as_i32(self) -> i32 { self as i32 }
            /// Parse from a stored integer; `None` if unknown.
            pub fn from_i32(v: i32) -> Option<Self> {
                match v { $($val => Some($name::$variant),)+ _ => None }
            }
        }
    };
}

int_enum! {
    /// How a note's body is stored/edited. (0 = plain is inferred; 1/2 observed.)
    BodyFormat { Plain = 0, Standard = 1, Markdown = 2 }
}

int_enum! {
    /// Where a note's displayed title comes from. (2 = date inferred.)
    TitleType { Auto = 0, Custom = 1, Date = 2 }
}

int_enum! {
    /// Desktop note display state.
    ViewType { Normal = 1, RolledUp = 2 }
}

int_enum! {
    /// Reminder repetition cadence (interval = RepeatEvery).
    RepeatType { None = 0, Daily = 1, Weekly = 2 }
}

int_enum! {
    /// Memoboard kind. Real folders: Folder/Root/Trash/Offline; the rest are virtual (smart) boards.
    MemoboardType {
        Folder = 0, Root = 1, Trash = 2,
        All = 3, Starred = 4, Due = 5, Unread = 6,
        OnDesktop = 7, OnWindows = 8,
        RecentlyCreated = 9, RecentlyModified = 10, RecentlyAccessed = 11,
        RecentlySent = 12, RecentlyReceived = 13,
        SharedByMe = 14, SharedWithMe = 15, SearchResults = 16,
        Offline = 19,
    }
}

impl MemoboardType {
    /// Smart/auto boards (query-driven) vs. real folders that hold notes.
    pub fn is_virtual(self) -> bool {
        !matches!(
            self,
            MemoboardType::Folder | MemoboardType::Root | MemoboardType::Trash | MemoboardType::Offline
        )
    }
}

/// A sticky note — the primary object. Placement that is inherently per-device (position/size)
/// lives here for the single-machine PoC; v2 moves it to a per-device overlay (see ARCHITECTURE).
#[derive(Debug, Clone)]
pub struct Note {
    pub id: Id,
    pub memoboard_id: Id,
    pub title: Option<String>,
    pub title_type: TitleType,
    pub body: String,
    pub body_format: BodyFormat,
    pub markdown_used: bool,
    /// Packed 0x00RRGGBB.
    pub color: u32,
    pub skin: Option<String>,
    /// 0..=255 (255 = opaque).
    pub transparency: u8,
    pub view_type: ViewType,
    pub position: (i32, i32),
    pub size: (i32, i32),
    pub stay_on_top: bool,
    pub stick_to_desktop: bool,
    pub locked: bool,
    pub completed: bool,
    pub starred: bool,
    pub checklist: bool,
    pub hidden: bool,
    pub auto_hide: bool,
    pub reminder_set: bool,
    pub created_on: TimestampMs,
    pub modified_on: TimestampMs,
    /// Content-only modification time; drives last-writer-wins on body/title during sync.
    pub modified_content_on: TimestampMs,
    pub accessed_on: TimestampMs,
    /// Tombstone for sync-friendly deletes; `None` = live.
    pub deleted_on: Option<TimestampMs>,
}

impl Note {
    /// A new note with sensible desktop-first defaults (Markdown body, stuck to desktop, opaque).
    pub fn new(id: impl Into<Id>, memoboard_id: impl Into<Id>, now: TimestampMs) -> Self {
        Note {
            id: id.into(),
            memoboard_id: memoboard_id.into(),
            title: None,
            title_type: TitleType::Auto,
            body: String::new(),
            body_format: BodyFormat::Markdown,
            markdown_used: true,
            color: 0x00FF_FF80, // soft sticky yellow
            skin: None,
            transparency: 255,
            view_type: ViewType::Normal,
            position: (0, 0),
            size: (240, 240),
            stay_on_top: false,
            stick_to_desktop: true,
            locked: false,
            completed: false,
            starred: false,
            checklist: false,
            hidden: false,
            auto_hide: false,
            reminder_set: false,
            created_on: now,
            modified_on: now,
            modified_content_on: now,
            accessed_on: now,
            deleted_on: None,
        }
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted_on.is_some()
    }
}

/// A folder or smart board.
#[derive(Debug, Clone)]
pub struct Memoboard {
    pub id: Id,
    pub name: String,
    pub parent_id: Option<Id>,
    pub board_type: MemoboardType,
    pub color: u32,
    pub favorite: bool,
    pub created_on: TimestampMs,
    pub modified_on: TimestampMs,
    pub deleted_on: Option<TimestampMs>,
}

/// An image/attachment belonging to a note. Write-once and immutable (a new insert = a new id),
/// referenced from note bodies as `pinleaf://object/<id>`. Synced as its own entity in v2.
#[derive(Debug, Clone)]
pub struct NoteObject {
    pub id: Id,
    pub note_id: Id,
    pub mime: String,
    pub bytes: Vec<u8>,
    pub created_on: TimestampMs,
    pub deleted_on: Option<TimestampMs>,
}

/// A reminder attached to a note (one per note).
#[derive(Debug, Clone)]
pub struct Reminder {
    pub note_id: Id,
    pub start_time: TimestampMs,
    pub repeat_type: RepeatType,
    pub repeat_every: i32,
    pub show_reminder: bool,
    pub play_sound: bool,
    pub advance_minutes: i32,
    pub executed: bool,
}
