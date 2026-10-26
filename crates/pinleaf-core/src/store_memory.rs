//! In-memory [`StoragePort`] — used by tests and early UI prototyping before/alongside the SQLite
//! impl. It mirrors the contract the SQLite store must satisfy (tombstones + change feed). It does
//! not enforce foreign keys, so tests may add notes without a backing board.

use std::collections::HashMap;

use crate::model::{Id, Memoboard, Note, NoteObject, Reminder, TimestampMs};
use crate::store::{Change, ChangeOp, EntityKind, StoragePort, StoreError, StoreResult};

#[derive(Default)]
pub struct MemoryStore {
    boards: HashMap<Id, Memoboard>,
    notes: HashMap<Id, Note>,
    reminders: HashMap<Id, Reminder>,
    tags: HashMap<Id, Vec<String>>,
    objects: HashMap<Id, NoteObject>,
    changes: Vec<Change>,
    seq: u64,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn record(&mut self, entity: EntityKind, id: &str, op: ChangeOp, at: TimestampMs) {
        self.seq += 1;
        self.changes.push(Change {
            seq: self.seq,
            entity,
            id: id.to_string(),
            op,
            at,
        });
    }
}

impl StoragePort for MemoryStore {
    fn put_memoboard(&mut self, board: &Memoboard) -> StoreResult<()> {
        self.boards.insert(board.id.clone(), board.clone());
        self.record(EntityKind::Memoboard, &board.id, ChangeOp::Upsert, board.modified_on);
        Ok(())
    }

    fn get_memoboard(&self, id: &str) -> StoreResult<Memoboard> {
        self.boards.get(id).cloned().ok_or(StoreError::NotFound)
    }

    fn list_memoboards(&self) -> StoreResult<Vec<Memoboard>> {
        Ok(self
            .boards
            .values()
            .filter(|b| b.deleted_on.is_none())
            .cloned()
            .collect())
    }

    fn put_note(&mut self, note: &Note) -> StoreResult<()> {
        self.notes.insert(note.id.clone(), note.clone());
        self.record(EntityKind::Note, &note.id, ChangeOp::Upsert, note.modified_on);
        Ok(())
    }

    fn get_note(&self, id: &str) -> StoreResult<Note> {
        self.notes.get(id).cloned().ok_or(StoreError::NotFound)
    }

    fn list_notes(&self) -> StoreResult<Vec<Note>> {
        Ok(self
            .notes
            .values()
            .filter(|n| !n.is_deleted())
            .cloned()
            .collect())
    }

    fn soft_delete_note(&mut self, id: &str, at: TimestampMs) -> StoreResult<()> {
        {
            let n = self.notes.get_mut(id).ok_or(StoreError::NotFound)?;
            n.deleted_on = Some(at);
            n.modified_on = at;
        }
        self.record(EntityKind::Note, id, ChangeOp::Delete, at);
        Ok(())
    }

    fn put_reminder(&mut self, reminder: &Reminder) -> StoreResult<()> {
        self.reminders
            .insert(reminder.note_id.clone(), reminder.clone());
        self.record(
            EntityKind::Reminder,
            &reminder.note_id,
            ChangeOp::Upsert,
            reminder.start_time,
        );
        Ok(())
    }

    fn get_reminder(&self, note_id: &str) -> StoreResult<Option<Reminder>> {
        Ok(self.reminders.get(note_id).cloned())
    }

    fn delete_reminder(&mut self, note_id: &str) -> StoreResult<()> {
        if self.reminders.remove(note_id).is_some() {
            self.record(EntityKind::Reminder, note_id, ChangeOp::Delete, 0);
        }
        Ok(())
    }

    fn due_reminders(&self, now: TimestampMs) -> StoreResult<Vec<Reminder>> {
        Ok(self
            .reminders
            .values()
            .filter(|r| {
                r.show_reminder
                    && !r.executed
                    && r.start_time <= now
                    && self.notes.get(&r.note_id).map_or(false, |n| !n.is_deleted())
            })
            .cloned()
            .collect())
    }

    fn put_object(&mut self, object: &NoteObject) -> StoreResult<()> {
        self.objects.insert(object.id.clone(), object.clone());
        self.record(EntityKind::Object, &object.id, ChangeOp::Upsert, object.created_on);
        Ok(())
    }

    fn get_object(&self, id: &str) -> StoreResult<Option<NoteObject>> {
        Ok(self
            .objects
            .get(id)
            .filter(|o| o.deleted_on.is_none())
            .cloned())
    }

    fn delete_object(&mut self, id: &str, at: TimestampMs) -> StoreResult<()> {
        if let Some(o) = self.objects.get_mut(id) {
            o.deleted_on = Some(at);
            self.record(EntityKind::Object, id, ChangeOp::Delete, at);
        }
        Ok(())
    }

    fn list_note_objects(&self, note_id: &str) -> StoreResult<Vec<NoteObject>> {
        Ok(self
            .objects
            .values()
            .filter(|o| o.note_id == note_id && o.deleted_on.is_none())
            .cloned()
            .collect())
    }

    fn set_note_tags(&mut self, note_id: &str, tags: &[String]) -> StoreResult<()> {
        // Tags are a sub-collection of the note; change-feed semantics for sync are TODO.
        self.tags.insert(note_id.to_string(), tags.to_vec());
        Ok(())
    }

    fn get_note_tags(&self, note_id: &str) -> StoreResult<Vec<String>> {
        Ok(self.tags.get(note_id).cloned().unwrap_or_default())
    }

    fn changes_since(&self, since: u64) -> StoreResult<Vec<Change>> {
        Ok(self
            .changes
            .iter()
            .filter(|c| c.seq > since)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BodyFormat, MemoboardType};

    #[test]
    fn put_get_list() {
        let mut s = MemoryStore::new();
        s.put_note(&Note::new("n1", "board", 1000)).unwrap();
        assert_eq!(s.get_note("n1").unwrap().id, "n1");
        assert_eq!(s.list_notes().unwrap().len(), 1);
    }

    #[test]
    fn soft_delete_hides_but_keeps_row_and_logs() {
        let mut s = MemoryStore::new();
        s.put_note(&Note::new("n1", "b", 1000)).unwrap();
        s.soft_delete_note("n1", 2000).unwrap();

        assert!(s.list_notes().unwrap().is_empty());
        assert!(s.get_note("n1").unwrap().is_deleted());

        let changes = s.changes_since(0).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].op, ChangeOp::Upsert);
        assert_eq!(changes[1].op, ChangeOp::Delete);
    }

    #[test]
    fn changes_since_is_a_cursor() {
        let mut s = MemoryStore::new();
        s.put_note(&Note::new("a", "b", 1)).unwrap();
        s.put_note(&Note::new("c", "b", 2)).unwrap();
        assert_eq!(s.changes_since(1).unwrap().len(), 1);
        assert_eq!(s.changes_since(2).unwrap().len(), 0);
    }

    #[test]
    fn enum_mapping_round_trips() {
        assert_eq!(MemoboardType::from_i32(7), Some(MemoboardType::OnDesktop));
        assert_eq!(MemoboardType::from_i32(17), None);
        assert!(MemoboardType::Starred.is_virtual());
        assert!(!MemoboardType::Folder.is_virtual());
        assert_eq!(BodyFormat::Markdown.as_i32(), 2);
        assert_eq!(BodyFormat::from_i32(1), Some(BodyFormat::Standard));
    }
}
