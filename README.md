# Pinleaf

Fast, skinnable **desktop sticky notes** — notes that live on your screen, with Markdown,
reminders, tags, images and a tray. Built with **Rust + Tauri v2**.

> Status: early but usable. Windows desktop proof-of-concept; architected to grow into a
> cross-platform, synced v2 (see [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)).

## Features
- **Desktop-first:** borderless, always-available sticky notes you operate on directly — the
  desktop is the app, not a manager window.
- **Markdown** notes with live render ↔ edit toggle, a formatting toolbar, tables, task-list
  checkboxes, and **images** (paste, drag-drop, or file picker).
- **Reminders** with due dates, repeat (daily/weekly), snooze, and OS notifications; overdue/soon
  cues on the note.
- **Organize:** colors, skins (Classic / Ruled / Grid / Gradient / Dark), tags.
- **Multi-note:** one window per note, positions/sizes remembered per note.
- **Tray + global hotkey** (`Ctrl+Alt+N` for a new note), and **Trash** with restore.

## Install (Windows)
Grab the installer from [Releases](https://github.com/blazejp83/pinleaf/releases) (an NSIS
`Pinleaf_<version>_x64-setup.exe`, per-user, no admin), or build from source below.

## Build from source
Prerequisites: [Rust](https://rustup.rs) (MSVC toolchain on Windows), Node.js, the
[Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) (on Windows: WebView2 — preinstalled
on Win10/11 — and the VS C++ build tools), and the Tauri CLI:

```sh
cargo install tauri-cli --version '^2' --locked
```

Then, from the repo root:

```sh
# run the desktop app in dev
cargo tauri dev --config crates/pinleaf-desktop/tauri.conf.json
# or build an installer
cargo tauri build --config crates/pinleaf-desktop/tauri.conf.json

# the portable core is plain Rust and tests anywhere:
cargo test -p pinleaf-core
```

The desktop shell targets Windows (deep desktop integration via WebView2). The `pinleaf-core`
crate is OS-agnostic and is what future mobile/web clients reuse.

## Repo layout
- `crates/pinleaf-core/` — portable domain core: entities, enums, and the `StoragePort`
  (local SQLite) with a change feed + tombstones designed for sync. No UI/OS deps; unit-tested.
- `crates/pinleaf-desktop/` — the Tauri v2 desktop shell (windows, tray, hotkeys, commands).
- `frontend/` — the webview UI (dependency-free HTML/CSS/JS).
- `docs/` — [`ARCHITECTURE.md`](docs/ARCHITECTURE.md) (local-first, sync-ready design),
  [`VISION.md`](docs/VISION.md), [`NOTE_UX.md`](docs/NOTE_UX.md).

## Architecture in one line
A pure-Rust core behind a storage port (local SQLite today), every entity carrying a stable id,
timestamps, a change-feed entry and a soft-delete tombstone — so v2 adds sync as a new storage
implementation rather than a rewrite. Details in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Clean-room note
Pinleaf is an independent, clean-room project inspired by the *concepts* of classic desktop
sticky-note apps. It contains no third-party product's code, assets, branding, or services, and is
not affiliated with or endorsed by any other product or company.

## License
MIT — see [`LICENSE`](LICENSE).
