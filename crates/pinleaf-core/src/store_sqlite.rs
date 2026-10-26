//! Local SQLite [`StoragePort`] (bundled SQLite; no system libsqlite3 needed).
//!
//! This is the v1 persistence. It enforces foreign keys, uses tombstones for deletes, and writes
//! the append-only change feed on every mutation — the same contract [`crate::MemoryStore`] keeps.

use rusqlite::{named_params, params, Connection, OptionalExtension, Row};

use crate::model::{
    BodyFormat, Memoboard, MemoboardType, Note, NoteObject, Reminder, RepeatType, TimestampMs,
    TitleType, ViewType,
};
use crate::store::{Change, ChangeOp, EntityKind, StoragePort, StoreError, StoreResult};

const MIGRATION: &str = include_str!("../migrations/0001_init.sql");

/// Applied idempotently on every open so existing databases gain the note_objects table too.
const MIGRATION_OBJECTS: &str = "\
CREATE TABLE IF NOT EXISTS note_objects (\
  id TEXT PRIMARY KEY NOT NULL,\
  note_id TEXT NOT NULL REFERENCES notes(id),\
  mime TEXT NOT NULL,\
  bytes BLOB NOT NULL,\
  created_on INTEGER NOT NULL,\
  deleted_on INTEGER\
);\
CREATE INDEX IF NOT EXISTS idx_objects_note ON note_objects(note_id);";

/// Column order shared by the note SELECTs and [`row_to_note`].
const NOTE_COLS: &str = "id, memoboard_id, title, title_type, body, body_format, markdown_used, \
    color, skin, transparency, view_type, position_x, position_y, size_x, size_y, stay_on_top, \
    stick_to_desktop, auto_hide, hidden, locked, completed, starred, checklist, reminder_set, \
    created_on, modified_on, modified_content_on, accessed_on, deleted_on";

const BOARD_COLS: &str =
    "id, name, parent_id, board_type, color, favorite, created_on, modified_on, deleted_on";

pub struct SqliteStore {
    conn: Connection,
}

fn backend<E: std::fmt::Display>(e: E) -> StoreError {
    StoreError::Backend(e.to_string())
}

impl SqliteStore {
    /// Open (creating if needed) a database at `path`, running the migration on first use.
    pub fn open(path: &str) -> StoreResult<Self> {
        let conn = Connection::open(path).map_err(backend)?;
        Self::init(conn)
    }

    /// An ephemeral in-memory database (tests, prototyping).
    pub fn open_in_memory() -> StoreResult<Self> {
        let conn = Connection::open_in_memory().map_err(backend)?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> StoreResult<Self> {
        conn.pragma_update(None, "foreign_keys", "ON").map_err(backend)?;
        let has_schema: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='app_meta'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)?;
        if has_schema.is_none() {
            conn.execute_batch(MIGRATION).map_err(backend)?;
        }
        conn.execute_batch(MIGRATION_OBJECTS).map_err(backend)?;
        Ok(Self { conn })
    }

    fn record(
        &self,
        entity: EntityKind,
        id: &str,
        op: ChangeOp,
        at: TimestampMs,
    ) -> StoreResult<()> {
        self.conn
            .execute(
                "INSERT INTO changes (entity, entity_id, op, at) VALUES (?1, ?2, ?3, ?4)",
                params![entity.as_str(), id, op.as_str(), at],
            )
            .map_err(backend)?;
        Ok(())
    }
}

fn row_to_note(r: &Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: r.get(0)?,
        memoboard_id: r.get(1)?,
        title: r.get(2)?,
        title_type: TitleType::from_i32(r.get(3)?).unwrap_or(TitleType::Auto),
        body: r.get(4)?,
        body_format: BodyFormat::from_i32(r.get(5)?).unwrap_or(BodyFormat::Standard),
        markdown_used: r.get(6)?,
        color: r.get(7)?,
        skin: r.get(8)?,
        transparency: r.get(9)?,
        view_type: ViewType::from_i32(r.get(10)?).unwrap_or(ViewType::Normal),
        position: (r.get(11)?, r.get(12)?),
        size: (r.get(13)?, r.get(14)?),
        stay_on_top: r.get(15)?,
        stick_to_desktop: r.get(16)?,
        auto_hide: r.get(17)?,
        hidden: r.get(18)?,
        locked: r.get(19)?,
        completed: r.get(20)?,
        starred: r.get(21)?,
        checklist: r.get(22)?,
        reminder_set: r.get(23)?,
        created_on: r.get(24)?,
        modified_on: r.get(25)?,
        modified_content_on: r.get(26)?,
        accessed_on: r.get(27)?,
        deleted_on: r.get(28)?,
    })
}

const REMINDER_COLS: &str =
    "note_id, start_time, repeat_type, repeat_every, show_reminder, advance_minutes, play_sound, executed";

fn row_to_reminder(r: &Row<'_>) -> rusqlite::Result<Reminder> {
    Ok(Reminder {
        note_id: r.get(0)?,
        start_time: r.get(1)?,
        repeat_type: RepeatType::from_i32(r.get(2)?).unwrap_or(RepeatType::None),
        repeat_every: r.get(3)?,
        show_reminder: r.get(4)?,
        advance_minutes: r.get(5)?,
        play_sound: r.get(6)?,
        executed: r.get(7)?,
    })
}

const OBJECT_COLS: &str = "id, note_id, mime, bytes, created_on, deleted_on";

fn row_to_object(r: &Row<'_>) -> rusqlite::Result<NoteObject> {
    Ok(NoteObject {
        id: r.get(0)?,
        note_id: r.get(1)?,
        mime: r.get(2)?,
        bytes: r.get(3)?,
        created_on: r.get(4)?,
        deleted_on: r.get(5)?,
    })
}

fn row_to_board(r: &Row<'_>) -> rusqlite::Result<Memoboard> {
    Ok(Memoboard {
        id: r.get(0)?,
        name: r.get(1)?,
        parent_id: r.get(2)?,
        board_type: MemoboardType::from_i32(r.get(3)?).unwrap_or(MemoboardType::Folder),
        color: r.get(4)?,
        favorite: r.get(5)?,
        created_on: r.get(6)?,
        modified_on: r.get(7)?,
        deleted_on: r.get(8)?,
    })
}

impl StoragePort for SqliteStore {
    fn put_memoboard(&mut self, b: &Memoboard) -> StoreResult<()> {
        self.conn
            .execute(
                "INSERT INTO memoboards (id, name, parent_id, board_type, color, favorite, \
                    created_on, modified_on, deleted_on) \
                 VALUES (:id, :name, :parent_id, :board_type, :color, :favorite, :created_on, \
                    :modified_on, :deleted_on) \
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name, parent_id=excluded.parent_id, \
                    board_type=excluded.board_type, color=excluded.color, favorite=excluded.favorite, \
                    modified_on=excluded.modified_on, deleted_on=excluded.deleted_on",
                named_params! {
                    ":id": b.id, ":name": b.name, ":parent_id": b.parent_id,
                    ":board_type": b.board_type.as_i32(), ":color": b.color, ":favorite": b.favorite,
                    ":created_on": b.created_on, ":modified_on": b.modified_on, ":deleted_on": b.deleted_on,
                },
            )
            .map_err(backend)?;
        self.record(EntityKind::Memoboard, &b.id, ChangeOp::Upsert, b.modified_on)
    }

    fn get_memoboard(&self, id: &str) -> StoreResult<Memoboard> {
        self.conn
            .query_row(
                &format!("SELECT {BOARD_COLS} FROM memoboards WHERE id=?1"),
                [id],
                row_to_board,
            )
            .optional()
            .map_err(backend)?
            .ok_or(StoreError::NotFound)
    }

    fn list_memoboards(&self) -> StoreResult<Vec<Memoboard>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {BOARD_COLS} FROM memoboards WHERE deleted_on IS NULL ORDER BY board_type, name"
            ))
            .map_err(backend)?;
        let rows = stmt
            .query_map([], row_to_board)
            .map_err(backend)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(backend)?;
        Ok(rows)
    }

    fn put_note(&mut self, n: &Note) -> StoreResult<()> {
        self.conn
            .execute(
                "INSERT INTO notes (id, memoboard_id, title, title_type, body, body_format, \
                    markdown_used, color, skin, transparency, view_type, position_x, position_y, \
                    size_x, size_y, stay_on_top, stick_to_desktop, auto_hide, hidden, locked, \
                    completed, starred, checklist, reminder_set, created_on, modified_on, \
                    modified_content_on, accessed_on, deleted_on) \
                 VALUES (:id, :memoboard_id, :title, :title_type, :body, :body_format, \
                    :markdown_used, :color, :skin, :transparency, :view_type, :position_x, \
                    :position_y, :size_x, :size_y, :stay_on_top, :stick_to_desktop, :auto_hide, \
                    :hidden, :locked, :completed, :starred, :checklist, :reminder_set, :created_on, \
                    :modified_on, :modified_content_on, :accessed_on, :deleted_on) \
                 ON CONFLICT(id) DO UPDATE SET memoboard_id=excluded.memoboard_id, \
                    title=excluded.title, title_type=excluded.title_type, body=excluded.body, \
                    body_format=excluded.body_format, markdown_used=excluded.markdown_used, \
                    color=excluded.color, skin=excluded.skin, transparency=excluded.transparency, \
                    view_type=excluded.view_type, position_x=excluded.position_x, \
                    position_y=excluded.position_y, size_x=excluded.size_x, size_y=excluded.size_y, \
                    stay_on_top=excluded.stay_on_top, stick_to_desktop=excluded.stick_to_desktop, \
                    auto_hide=excluded.auto_hide, hidden=excluded.hidden, locked=excluded.locked, \
                    completed=excluded.completed, starred=excluded.starred, checklist=excluded.checklist, \
                    reminder_set=excluded.reminder_set, modified_on=excluded.modified_on, \
                    modified_content_on=excluded.modified_content_on, accessed_on=excluded.accessed_on, \
                    deleted_on=excluded.deleted_on",
                named_params! {
                    ":id": n.id, ":memoboard_id": n.memoboard_id, ":title": n.title,
                    ":title_type": n.title_type.as_i32(), ":body": n.body,
                    ":body_format": n.body_format.as_i32(), ":markdown_used": n.markdown_used,
                    ":color": n.color, ":skin": n.skin, ":transparency": n.transparency,
                    ":view_type": n.view_type.as_i32(), ":position_x": n.position.0,
                    ":position_y": n.position.1, ":size_x": n.size.0, ":size_y": n.size.1,
                    ":stay_on_top": n.stay_on_top, ":stick_to_desktop": n.stick_to_desktop,
                    ":auto_hide": n.auto_hide, ":hidden": n.hidden, ":locked": n.locked,
                    ":completed": n.completed, ":starred": n.starred, ":checklist": n.checklist,
                    ":reminder_set": n.reminder_set, ":created_on": n.created_on,
                    ":modified_on": n.modified_on, ":modified_content_on": n.modified_content_on,
                    ":accessed_on": n.accessed_on, ":deleted_on": n.deleted_on,
                },
            )
            .map_err(backend)?;
        self.record(EntityKind::Note, &n.id, ChangeOp::Upsert, n.modified_on)
    }

    fn get_note(&self, id: &str) -> StoreResult<Note> {
        self.conn
            .query_row(
                &format!("SELECT {NOTE_COLS} FROM notes WHERE id=?1"),
                [id],
                row_to_note,
            )
            .optional()
            .map_err(backend)?
            .ok_or(StoreError::NotFound)
    }

    fn list_notes(&self) -> StoreResult<Vec<Note>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {NOTE_COLS} FROM notes WHERE deleted_on IS NULL ORDER BY note_order"
            ))
            .map_err(backend)?;
        let rows = stmt
            .query_map([], row_to_note)
            .map_err(backend)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(backend)?;
        Ok(rows)
    }

    fn soft_delete_note(&mut self, id: &str, at: TimestampMs) -> StoreResult<()> {
        let affected = self
            .conn
            .execute(
                "UPDATE notes SET deleted_on=?1, modified_on=?1 WHERE id=?2 AND deleted_on IS NULL",
                params![at, id],
            )
            .map_err(backend)?;
        if affected == 0 {
            return Err(StoreError::NotFound);
        }
        self.record(EntityKind::Note, id, ChangeOp::Delete, at)
    }

    fn put_reminder(&mut self, rm: &Reminder) -> StoreResult<()> {
        self.conn
            .execute(
                "INSERT INTO reminders (note_id, start_time, repeat_type, repeat_every, \
                    show_reminder, advance_minutes, play_sound, executed) \
                 VALUES (:note_id, :start_time, :repeat_type, :repeat_every, :show_reminder, \
                    :advance_minutes, :play_sound, :executed) \
                 ON CONFLICT(note_id) DO UPDATE SET start_time=excluded.start_time, \
                    repeat_type=excluded.repeat_type, repeat_every=excluded.repeat_every, \
                    show_reminder=excluded.show_reminder, advance_minutes=excluded.advance_minutes, \
                    play_sound=excluded.play_sound, executed=excluded.executed",
                named_params! {
                    ":note_id": rm.note_id, ":start_time": rm.start_time,
                    ":repeat_type": rm.repeat_type.as_i32(), ":repeat_every": rm.repeat_every,
                    ":show_reminder": rm.show_reminder, ":advance_minutes": rm.advance_minutes,
                    ":play_sound": rm.play_sound, ":executed": rm.executed,
                },
            )
            .map_err(backend)?;
        self.record(EntityKind::Reminder, &rm.note_id, ChangeOp::Upsert, rm.start_time)
    }

    fn get_reminder(&self, note_id: &str) -> StoreResult<Option<Reminder>> {
        self.conn
            .query_row(
                &format!("SELECT {REMINDER_COLS} FROM reminders WHERE note_id=?1"),
                [note_id],
                row_to_reminder,
            )
            .optional()
            .map_err(backend)
    }

    fn delete_reminder(&mut self, note_id: &str) -> StoreResult<()> {
        let affected = self
            .conn
            .execute("DELETE FROM reminders WHERE note_id=?1", [note_id])
            .map_err(backend)?;
        if affected > 0 {
            self.record(EntityKind::Reminder, note_id, ChangeOp::Delete, 0)?;
        }
        Ok(())
    }

    fn due_reminders(&self, now: TimestampMs) -> StoreResult<Vec<Reminder>> {
        let sql = format!(
            "SELECT {REMINDER_COLS} FROM reminders \
             JOIN notes ON notes.id = reminders.note_id \
             WHERE notes.deleted_on IS NULL AND reminders.executed = 0 \
               AND reminders.show_reminder = 1 AND reminders.start_time <= ?1 \
             ORDER BY reminders.start_time"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(backend)?;
        let rows = stmt
            .query_map([now], row_to_reminder)
            .map_err(backend)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(backend)?;
        Ok(rows)
    }

    fn set_note_tags(&mut self, note_id: &str, tags: &[String]) -> StoreResult<()> {
        let tx = self.conn.transaction().map_err(backend)?;
        tx.execute("DELETE FROM note_tags WHERE note_id=?1", [note_id])
            .map_err(backend)?;
        for (i, name) in tags.iter().enumerate() {
            tx.execute(
                "INSERT INTO note_tags (note_id, name, tag_order) VALUES (?1, ?2, ?3)",
                params![note_id, name, i as i64],
            )
            .map_err(backend)?;
        }
        tx.commit().map_err(backend)?;
        // Change-feed semantics for tag sync are TODO (sub-collection of the note).
        Ok(())
    }

    fn get_note_tags(&self, note_id: &str) -> StoreResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM note_tags WHERE note_id=?1 ORDER BY tag_order")
            .map_err(backend)?;
        let rows = stmt
            .query_map([note_id], |r| r.get::<_, String>(0))
            .map_err(backend)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(backend)?;
        Ok(rows)
    }

    fn put_object(&mut self, o: &NoteObject) -> StoreResult<()> {
        self.conn
            .execute(
                "INSERT INTO note_objects (id, note_id, mime, bytes, created_on, deleted_on) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![o.id, o.note_id, o.mime, o.bytes, o.created_on, o.deleted_on],
            )
            .map_err(backend)?;
        self.record(EntityKind::Object, &o.id, ChangeOp::Upsert, o.created_on)
    }

    fn get_object(&self, id: &str) -> StoreResult<Option<NoteObject>> {
        self.conn
            .query_row(
                &format!("SELECT {OBJECT_COLS} FROM note_objects WHERE id=?1 AND deleted_on IS NULL"),
                [id],
                row_to_object,
            )
            .optional()
            .map_err(backend)
    }

    fn delete_object(&mut self, id: &str, at: TimestampMs) -> StoreResult<()> {
        let affected = self
            .conn
            .execute(
                "UPDATE note_objects SET deleted_on=?1 WHERE id=?2 AND deleted_on IS NULL",
                params![at, id],
            )
            .map_err(backend)?;
        if affected > 0 {
            self.record(EntityKind::Object, id, ChangeOp::Delete, at)?;
        }
        Ok(())
    }

    fn list_note_objects(&self, note_id: &str) -> StoreResult<Vec<NoteObject>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {OBJECT_COLS} FROM note_objects WHERE note_id=?1 AND deleted_on IS NULL ORDER BY created_on"
            ))
            .map_err(backend)?;
        let rows = stmt
            .query_map([note_id], row_to_object)
            .map_err(backend)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(backend)?;
        Ok(rows)
    }

    fn changes_since(&self, since: u64) -> StoreResult<Vec<Change>> {
        let mut stmt = self
            .conn
            .prepare("SELECT seq, entity, entity_id, op, at FROM changes WHERE seq > ?1 ORDER BY seq")
            .map_err(backend)?;
        let rows = stmt
            .query_map([since as i64], |r| {
                let seq: i64 = r.get(0)?;
                let entity: String = r.get(1)?;
                let id: String = r.get(2)?;
                let op: String = r.get(3)?;
                let at: i64 = r.get(4)?;
                Ok((seq, entity, id, op, at))
            })
            .map_err(backend)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(backend)?;

        let mut out = Vec::with_capacity(rows.len());
        for (seq, entity, id, op, at) in rows {
            out.push(Change {
                seq: seq as u64,
                entity: EntityKind::from_str(&entity)
                    .ok_or_else(|| StoreError::Backend(format!("bad entity {entity:?}")))?,
                id,
                op: ChangeOp::from_str(&op)
                    .ok_or_else(|| StoreError::Backend(format!("bad op {op:?}")))?,
                at,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MemoboardType;

    fn board(id: &str) -> Memoboard {
        Memoboard {
            id: id.into(),
            name: "Board".into(),
            parent_id: None,
            board_type: MemoboardType::Folder,
            color: 0,
            favorite: false,
            created_on: 10,
            modified_on: 10,
            deleted_on: None,
        }
    }

    #[test]
    fn crud_round_trip() {
        let mut s = SqliteStore::open_in_memory().unwrap();
        s.put_memoboard(&board("b1")).unwrap();

        let mut n = Note::new("n1", "b1", 1000);
        n.title = Some("Hello".into());
        n.body = "world".into();
        n.body_format = BodyFormat::Markdown;
        n.position = (12, 34);
        n.starred = true;
        s.put_note(&n).unwrap();

        let got = s.get_note("n1").unwrap();
        assert_eq!(got.title.as_deref(), Some("Hello"));
        assert_eq!(got.body, "world");
        assert_eq!(got.body_format, BodyFormat::Markdown);
        assert_eq!(got.position, (12, 34));
        assert!(got.starred);
        assert_eq!(s.list_notes().unwrap().len(), 1);
        assert_eq!(s.list_memoboards().unwrap().len(), 1);
    }

    #[test]
    fn foreign_key_is_enforced() {
        let mut s = SqliteStore::open_in_memory().unwrap();
        // No board "missing" exists -> FK violation.
        let err = s.put_note(&Note::new("n1", "missing", 1));
        assert!(matches!(err, Err(StoreError::Backend(_))), "expected FK error, got {err:?}");
    }

    #[test]
    fn soft_delete_and_change_feed() {
        let mut s = SqliteStore::open_in_memory().unwrap();
        s.put_memoboard(&board("b1")).unwrap();
        s.put_note(&Note::new("n1", "b1", 1000)).unwrap();
        s.soft_delete_note("n1", 2000).unwrap();

        assert!(s.list_notes().unwrap().is_empty());
        assert!(s.get_note("n1").unwrap().is_deleted());
        assert!(matches!(s.soft_delete_note("nope", 3000), Err(StoreError::NotFound)));

        let changes = s.changes_since(0).unwrap();
        // board upsert, note upsert, note delete
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].entity, EntityKind::Memoboard);
        assert_eq!(changes[2].op, ChangeOp::Delete);
        assert_eq!(changes[2].id, "n1");
        // cursor
        assert_eq!(s.changes_since(changes[2].seq).unwrap().len(), 0);
    }

    #[test]
    fn reminders_and_tags() {
        use crate::model::RepeatType;
        let mut s = SqliteStore::open_in_memory().unwrap();
        s.put_memoboard(&board("b1")).unwrap();
        s.put_note(&Note::new("n1", "b1", 1000)).unwrap();

        assert!(s.get_reminder("n1").unwrap().is_none());
        s.put_reminder(&Reminder {
            note_id: "n1".into(),
            start_time: 1_700_000_000_000,
            repeat_type: RepeatType::None,
            repeat_every: 0,
            show_reminder: true,
            play_sound: true,
            advance_minutes: 15,
            executed: false,
        })
        .unwrap();
        let r = s.get_reminder("n1").unwrap().unwrap();
        assert_eq!(r.start_time, 1_700_000_000_000);
        assert_eq!(r.advance_minutes, 15);
        s.delete_reminder("n1").unwrap();
        assert!(s.get_reminder("n1").unwrap().is_none());

        s.set_note_tags("n1", &["work".into(), "urgent".into()]).unwrap();
        assert_eq!(s.get_note_tags("n1").unwrap(), vec!["work", "urgent"]);
        // replacing keeps order and drops removed tags
        s.set_note_tags("n1", &["later".into()]).unwrap();
        assert_eq!(s.get_note_tags("n1").unwrap(), vec!["later"]);
    }

    #[test]
    fn due_reminders_filters_future_executed_and_deleted() {
        use crate::model::RepeatType;
        let mk = |id: &str, start: i64, executed: bool| Reminder {
            note_id: id.into(),
            start_time: start,
            repeat_type: RepeatType::None,
            repeat_every: 0,
            show_reminder: true,
            play_sound: false,
            advance_minutes: 0,
            executed,
        };
        let mut s = SqliteStore::open_in_memory().unwrap();
        s.put_memoboard(&board("b1")).unwrap();
        for id in ["due", "future", "done", "gone"] {
            s.put_note(&Note::new(id, "b1", 0)).unwrap();
        }
        s.put_reminder(&mk("due", 1_000, false)).unwrap();
        s.put_reminder(&mk("future", 9_999, false)).unwrap();
        s.put_reminder(&mk("done", 1_000, true)).unwrap();
        s.put_reminder(&mk("gone", 1_000, false)).unwrap();
        s.soft_delete_note("gone", 2_000).unwrap();

        let due = s.due_reminders(5_000).unwrap();
        let ids: Vec<_> = due.iter().map(|r| r.note_id.as_str()).collect();
        assert_eq!(ids, vec!["due"]); // future(not yet), done(executed), gone(deleted) excluded
    }

    #[test]
    fn note_objects_store_and_tombstone() {
        let mut s = SqliteStore::open_in_memory().unwrap();
        s.put_memoboard(&board("b1")).unwrap();
        s.put_note(&Note::new("n1", "b1", 0)).unwrap();
        let obj = NoteObject {
            id: "img1".into(),
            note_id: "n1".into(),
            mime: "image/png".into(),
            bytes: vec![1, 2, 3, 4],
            created_on: 100,
            deleted_on: None,
        };
        s.put_object(&obj).unwrap();
        let got = s.get_object("img1").unwrap().unwrap();
        assert_eq!(got.mime, "image/png");
        assert_eq!(got.bytes, vec![1, 2, 3, 4]);
        assert_eq!(s.list_note_objects("n1").unwrap().len(), 1);
        s.delete_object("img1", 200).unwrap();
        assert!(s.get_object("img1").unwrap().is_none());
        assert!(s.list_note_objects("n1").unwrap().is_empty());
    }

    #[test]
    fn data_persists_across_reopen() {
        let path = std::env::temp_dir().join(format!(
            "pinleaf_test_{}_{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let p = path.to_str().unwrap().to_string();

        {
            let mut s = SqliteStore::open(&p).unwrap();
            s.put_memoboard(&board("b1")).unwrap();
            let mut n = Note::new("n1", "b1", 1000);
            n.body = "persist me".into();
            s.put_note(&n).unwrap();
        } // drop closes the connection

        {
            let s = SqliteStore::open(&p).unwrap();
            let got = s.get_note("n1").unwrap();
            assert_eq!(got.body, "persist me");
        }

        // best-effort cleanup (incl. WAL sidecars)
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(format!("{p}-wal"));
        let _ = std::fs::remove_file(format!("{p}-shm"));
    }
}
