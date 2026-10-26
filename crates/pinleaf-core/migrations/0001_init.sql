-- Pinleaf schema v1 (clean-room). SQLite. One time unit: Unix epoch MILLISECONDS (INTEGER).
-- Ids are client-generated UUID strings. Deletes are tombstones (deleted_on) for sync safety.
-- This is our own schema design.

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE memoboards (
    id            TEXT PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL,
    parent_id     TEXT REFERENCES memoboards(id),
    board_type    INTEGER NOT NULL DEFAULT 0,   -- MemoboardType (0 folder,1 root,2 trash,3+ virtual,19 offline)
    color         INTEGER NOT NULL DEFAULT 0,
    favorite      INTEGER NOT NULL DEFAULT 0,
    hotkey        INTEGER NOT NULL DEFAULT 0,
    note_order    INTEGER NOT NULL DEFAULT 0,
    list_layout   TEXT,                          -- JSON: [{InternalName,Order,Width,Visible}]
    created_on    INTEGER NOT NULL,
    modified_on   INTEGER NOT NULL,
    deleted_on    INTEGER                        -- tombstone (NULL = live)
);

CREATE TABLE notes (
    id                   TEXT PRIMARY KEY NOT NULL,
    memoboard_id         TEXT NOT NULL REFERENCES memoboards(id),
    title                TEXT,
    title_type           INTEGER NOT NULL DEFAULT 0,  -- 0 auto, 1 custom, 2 date
    body                 TEXT NOT NULL DEFAULT '',
    body_format          INTEGER NOT NULL DEFAULT 2,  -- 0 plain, 1 standard/RTF, 2 markdown
    markdown_used        INTEGER NOT NULL DEFAULT 1,
    color                INTEGER NOT NULL DEFAULT 16777088, -- 0x00FFFF80
    skin                 TEXT,
    transparency         INTEGER NOT NULL DEFAULT 255,
    view_type            INTEGER NOT NULL DEFAULT 1,   -- 1 normal, 2 rolled up
    position_x           INTEGER NOT NULL DEFAULT 0,
    position_y           INTEGER NOT NULL DEFAULT 0,
    size_x               INTEGER NOT NULL DEFAULT 240,
    size_y               INTEGER NOT NULL DEFAULT 240,
    size_to_content      INTEGER NOT NULL DEFAULT 0,
    stay_on_top          INTEGER NOT NULL DEFAULT 0,
    stick_to_desktop     INTEGER NOT NULL DEFAULT 1,
    auto_hide            INTEGER NOT NULL DEFAULT 0,
    hidden               INTEGER NOT NULL DEFAULT 0,
    locked               INTEGER NOT NULL DEFAULT 0,
    completed            INTEGER NOT NULL DEFAULT 0,
    starred              INTEGER NOT NULL DEFAULT 0,
    checklist            INTEGER NOT NULL DEFAULT 0,
    reminder_set         INTEGER NOT NULL DEFAULT 0,
    note_order           INTEGER NOT NULL DEFAULT 0,
    created_on           INTEGER NOT NULL,
    modified_on          INTEGER NOT NULL,
    modified_content_on  INTEGER NOT NULL,           -- drives last-writer-wins on body/title
    accessed_on          INTEGER NOT NULL,
    deleted_on           INTEGER                      -- tombstone
);
CREATE INDEX idx_notes_board ON notes(memoboard_id);
CREATE INDEX idx_notes_live  ON notes(deleted_on);

CREATE TABLE reminders (
    note_id         TEXT PRIMARY KEY NOT NULL REFERENCES notes(id),
    start_time      INTEGER NOT NULL,
    end_time        INTEGER,
    repeat_type     INTEGER NOT NULL DEFAULT 0,       -- 0 none, 1 repeating
    repeat_every    INTEGER NOT NULL DEFAULT 0,
    repeat_param    INTEGER NOT NULL DEFAULT 0,
    show_reminder   INTEGER NOT NULL DEFAULT 1,
    advance_minutes INTEGER NOT NULL DEFAULT 0,
    play_sound      INTEGER NOT NULL DEFAULT 1,
    sound_path      TEXT,
    snooze_until    INTEGER,
    executed        INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE note_tags (
    note_id    TEXT NOT NULL REFERENCES notes(id),
    name       TEXT NOT NULL,
    tag_order  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (note_id, name)
);

CREATE TABLE note_links (
    id        INTEGER PRIMARY KEY,
    note_id   TEXT NOT NULL REFERENCES notes(id),
    link_type INTEGER NOT NULL,                       -- file / folder / website / window
    name      TEXT,
    address   TEXT NOT NULL
);

-- Append-only change feed: undo/history now, sync delta source later.
CREATE TABLE changes (
    seq        INTEGER PRIMARY KEY AUTOINCREMENT,
    entity     TEXT NOT NULL,                          -- 'note' | 'memoboard' | 'reminder'
    entity_id  TEXT NOT NULL,
    op         TEXT NOT NULL,                          -- 'upsert' | 'delete'
    at         INTEGER NOT NULL,
    device_id  TEXT
);
CREATE INDEX idx_changes_entity ON changes(entity, entity_id);

-- Key/value for app settings and sync cursors (mirrors the original's UserSettings JSON model).
CREATE TABLE app_meta (
    key         TEXT PRIMARY KEY NOT NULL,
    value       TEXT,
    modified_on INTEGER
);

INSERT INTO app_meta(key, value, modified_on) VALUES ('schema_version', '1', 0);
