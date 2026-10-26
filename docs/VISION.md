# Pinleaf — product vision

Pinleaf keeps the concepts that make desktop sticky-notes great and fixes the parts that feel
dated.

## Design pillar: desktop-first
The desktop is the app. The classic model centers a manager window where you pick which notes to
display; Pinleaf inverts that — **the notes on the desktop are the primary surface**, and you do
nearly everything on the note itself (view due dates, edit, set details via a hover ⚙ popover).
The manager window is a secondary tool for search and bulk work. Full interaction model in
`NOTE_UX.md`.

## Keep (the genuinely good concepts)
- **Real desktop sticky notes**: borderless skinned notes that live on the desktop, stay on top,
  stick to the wallpaper, roll up, go transparent.
- **Stick-to-window / stick-to-folder / stick-to-website**: a note that pops up in context.
- **Memoboards**: nested folders + smart/virtual boards (All, Starred, Due, Recent, …).
- **Reminders** on notes, with repeat/snooze/advance.
- **Tags, checklists, colors, skins.**

## Improve (where we go beyond)
1. **Markdown-first editor with live preview** — Markdown (with checklists, tables, highlights) as
   the primary format, stored as portable text, with an optional rich toolbar. No lock-in to RTF
   blobs.
2. **Instant full-text search** across all notes (FTS), with filters (tag, board, color, has-
   reminder) and a results board. Search should be the fastest way to any note.
3. **Command palette + keyboard-first UX** — create, find, move, tag, set reminder without the
   mouse.
4. **CSS/theme-based skinning**: skins are declarative (colors, gradients, backgrounds, 9-slice
   images) and themeable for light/dark automatically.
5. **Open, exportable data** — notes export to Markdown/JSON; the DB is inspectable; no
   proprietary lock-in. Easy backup/restore and import.
6. **Cross-platform + sync from the architecture up** (see `ARCHITECTURE.md`) with an
   **end-to-end-encryptable** option — your notes on every device, private by default.
7. **Quick capture** — a global hotkey to a capture box, web clipper later, so notes get in
   frictionlessly.
8. **Better reminders** — natural-language due dates ("next fri 3pm"), snooze presets, and a
   unified "Due" view.
9. **Accessibility & HiDPI done right** — crisp at any scale, screen-reader friendly, respects OS
   reduced-motion/contrast.

## Explicitly out of scope for v1
Sync/accounts, mobile/web shells, LAN note-sending, contacts/sharing. All are anticipated by the
architecture but not built in v1.
