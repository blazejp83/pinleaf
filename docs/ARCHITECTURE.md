# Pinleaf architecture — local now, sync-ready later

The #1 constraint: **v1 is a local PoC, but nothing in v1 should have to be thrown away to add
v2 sync and extra platforms.** We get that by drawing the right seams now and following a
local-first design, even while the only backend is a local SQLite file.

## Layers (keep these boundaries from day one)
```
┌──────────────────────────────────────────────┐
│ UI shells (per platform)                       │  desktop PoC now; mobile/web later
│  - desktop note windows, notes browser, tray   │
├──────────────────────────────────────────────┤
│ Application / services                          │  note CRUD, reminders scheduler,
│  (platform-agnostic use-cases)                  │  search, skin rendering, hotkeys
├──────────────────────────────────────────────┤
│ Core domain (pure, portable)                    │  entities + enums + rules; NO UI, NO SQL
├──────────────────────────────────────────────┤
│ Storage port (interface)                        │  trait/interface: get/put/query/changes
│   └─ LocalSqliteStore (v1 impl)                 │  v2 adds SyncingStore wrapping the same port
└──────────────────────────────────────────────┘
```
Rules:
- The **core domain** is pure and has no dependency on SQLite, the UI, or the OS.
- All persistence goes through a **storage port** (interface). v1 ships one implementation
  (local SQLite). v2 adds a sync engine *behind the same port* — the app/services layer doesn't
  change.
- UI shells depend on the application layer, never on storage directly. Adding a platform = a new
  shell, not a new core.

## Stack (decided): Rust + Tauri v2
- **Core crate** (`pinleaf-core`): pure Rust domain + services + storage port + local SQLite impl
  (via `rusqlite`/`sqlx`). UI-agnostic, no Tauri deps — this is the piece v2 mobile/web reuse.
- **Desktop shell**: Tauri v2 app. Rust backend (`src-tauri`) hosts `pinleaf-core` and owns the
  OS-level desktop integration; the webview renders note windows and the manager UI.
- **Frontend**: a lightweight web UI in the webview (framework TBD — lean, e.g. Svelte/Solid or
  plain TS; notes are small surfaces, keep it fast). Markdown rendering client-side.
- Each **desktop note is its own Tauri window** (borderless, transparent, always-on-top as
  needed); the manager is another window; a tray icon + global shortcuts via Tauri plugins.

### Desktop integration & build target
- Pinleaf's value is OS-level: borderless/transparent/topmost windows, stick-to-desktop (wallpaper
  layer), stick-to-window (track another app's window), global hotkeys, tray. These are
  **platform-specific** and must run on the real target OS.
- **PoC target = Windows** (Tauri uses the preinstalled WebView2; Rust via MSVC). Building a Linux
  app in WSL is the wrong target and is not how we test desktop behavior.
- Some effects (true stick-to-wallpaper, per-window click-through, following another window) may
  need platform APIs beyond Tauri's cross-platform set — isolate these behind a small
  `desktop-integration` trait in the shell, with a Windows impl first (Win32 via `windows` crate),
  so other platforms get their own impl in v2 without touching core/services.

## Data model decisions that make sync cheap later
These are in v1 even though v1 doesn't sync, because retrofitting them is painful:
1. **Stable GUID primary keys** for every entity (not autoincrement). Already true in the schema
   — ids are client-generated UUIDs, so two devices never collide.
2. **Timestamps on everything**: `CreatedOn`, `ModifiedOn`, `ModifiedContentOn`, `AccessedOn`
   (Unix ms in app logic; see units note below). Enables last-writer-wins and "changed since
   LastSyncTime" deltas.
3. **Soft delete / tombstones**: keep a `PreDelete`/deleted marker + deletedOn instead of hard
   DELETE, so deletions can propagate on sync and the Trash board works. Purge tombstones only
   after they've synced everywhere.
4. **Per-entity change tracking**: an append-only change log (one row per create/update/delete
   with entity id, type, timestamp, device id). v1 can use it for undo/history; v2 uses it as the
   sync delta feed.
5. **Device identity + LastSyncTime per entity-type**. Stored even in v1 (just always
   "never synced").
6. **A `userId`/owner field** on syncable rows (nullable/local in v1) so multi-account and
   sharing slot in without a migration.

## Sync design (v2 sketch — do not build yet, just don't preclude it)
- **Transport:** our own service; client pulls `changes since <LastSyncTime>` per entity type and
  pushes local changes; server stores per-user entity versions.
- **Conflict policy:** start with **last-writer-wins by `ModifiedContentOn`** at the field-group
  level (title/body/appearance/placement as separate groups so moving a note and editing its text
  on two devices both survive). Leave room to upgrade note-body merging to CRDT later.
- **Deletes:** tombstone propagation; client reconciles, then purges.
- **Privacy:** design payloads so bodies can be **end-to-end encrypted** (server stores
  ciphertext + metadata). Keep searchable metadata separate from encrypted content.
- **Placement is device-specific:** note window position/size/skin-scale belong in a per-device
  overlay, not the synced note, so a note can sit in different spots on each screen. Keep that
  split in v1.

## Images / attachments (sync-ready from day one)
Images are **first-class entities**, not inline blobs in note text or device-local files — so v2
sync reuses the existing entity+change-feed mechanism with no new channel:
- A `NoteObject { id, note_id, mime, bytes, created_on, deleted_on }` row holds the image bytes.
  It emits change-feed rows on create/delete and uses
  a `deleted_on` tombstone — identical to notes/reminders, so the sync engine syncs it the same way.
- Note bodies reference images by a **stable, device-independent id**: Markdown
  `![alt](pinleaf://object/<id>)`. No local paths/URLs in note content, so references stay valid on
  every device once the object has synced.
- Objects are **write-once / immutable** (a new paste = a new id) → image content never
  merge-conflicts; only deletion (tombstone) propagates. Content-hash ids can replace GUIDs later
  for dedup without changing the shape.
- The whole object row (metadata + bytes) is one self-contained, **end-to-end-encryptable** unit.
- The desktop shell serves bytes to the webview via a registered `pinleaf://object/<id>` URI-scheme
  handler that reads from the store; on any device post-sync the same id resolves locally.

Rejected: inline base64 (bloats every note's sync payload, no dedup) and local files + asset
protocol (needs a separate file-sync channel in v2 — the rewrite we're avoiding).

## Timestamp units
Pinleaf uses **Unix milliseconds everywhere** internally; only convert at the SQLite boundary if
we keep integer columns. One unit, no surprises.

## PoC scope guardrails
Build v1 lean, but through the seams above:
- Local SQLite via the storage port; GUID ids; soft-delete; change log written (even if only used
  for undo in v1).
- One desktop shell; note window + notes browser + a couple of skins + reminders.
- No account, no network. The sync engine is a *future* storage-port decorator.
