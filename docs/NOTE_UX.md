# Pinleaf note UX — desktop-first interaction model

**Design pillar: the desktop is the app.** Unlike the classic model (a main manager window where
you pick which notes to show), in Pinleaf the **notes on the desktop are the primary surface**.
You create, read, edit, and manage notes directly on the note. The manager/browser window is a
*secondary* tool for search, bulk ops, and boards — not the main stage.

Everything below is a **hypothesis to prototype and test**, not a locked spec. The point is to
try operating entirely from the notes and only fall back to a manager window when needed.

## The note is the primary object
- Create: global hotkey or tray → a new note appears on the desktop, cursor in the body, ready to
  type. No dialog, no window-picking step.
- Edit: click (or double-click — configurable) into the body; inline, in place.
- Manage: per-note affordances live *on the note*, revealed on hover, so the note stays clean when
  idle.

## On-note layout (top → bottom)
```
┌───────────────────────────────┐
│ ⋯title bar (skin)      ⚙ ◻ ✕  │  ← hover affordances: settings, roll-up, close
│  [Title]  (if a title is set) │
│  ⏰ Fri 3:00 PM   #tags        │  ← meta strip: due date / reminder, tags (only if present)
│  ─────────────────────────────│
│  body (markdown / rich text)  │
│                               │
│  ○ checklist item             │
└───────────────────────────────┘
```
- **Due date / reminder shown on the note itself** — a compact meta strip directly **below the
  title** (or at the very top if no title is set). Color/urgency cue as it approaches (e.g. amber
  when due today, red when overdue). This replaces having to open a dialog to see a note is due.
- **Title** is optional (`TitleType`: auto from first body line vs custom). When auto, the meta
  strip sits at the top.
- **Tags** render as small chips in the meta strip when present.
- The meta strip only appears when there's something to show (due date, tags, lock, shared).

## Hover affordances (kept off until hover, to stay clean)
- **⚙ settings icon** on the note → opens a lightweight inline popover (not a big modal) for that
  note's details: color/skin, reminder/due date, tags, lock, stick-to-window, stay-on-top,
  transparency, move-to-board. This is where "note details" are set — per the idea of a small
  settings icon revealed on hover.
- **roll-up / collapse** toggle (ViewType) and **close/hide** on the title bar.
- **resize** handle at a corner; **drag** anywhere on the title bar to move.
- A small **＋** to spawn a linked/new note, and a **⋯ overflow** for less-common actions
  (duplicate, export, delete, send).

## Interactions to prototype & compare
- Settings via hover ⚙ popover vs. right-click context menu vs. a slide-out edge panel on the note.
- Due date: inline natural-language entry on the meta strip ("next fri 3pm") vs. a date picker.
- Single-click vs double-click to edit (a configurable behavior).
- Quick actions via a per-note mini command (press a key while the note is focused).
- Checklist mode toggled from the note, not a dialog.

## Manager window (secondary, still useful)
Full-text search, boards/memoboards tree, bulk tag/move/delete, a "Due" overview, import/export.
It operates on the same notes; it is not where day-to-day note use happens.

## Why this is better
- Zero-friction capture and triage: see what's due at a glance, act on the note in place.
- The desktop becomes a spatial workspace (notes where you left them) rather than a list you
  render from a hub.
- Fewer modal dialogs; details edited inline, contextually.
