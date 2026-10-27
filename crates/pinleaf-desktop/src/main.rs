#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Pinleaf desktop shell (Tauri v2). Desktop-first: one borderless window per note (window label =
//! note id), an on-note settings popover, a tray menu, and a global new-note shortcut. Notes are
//! backed by `pinleaf-core`'s SQLite store. See `docs/NOTE_UX.md`, `docs/ARCHITECTURE.md`.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, LogicalSize, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent, Wry,
};
use tauri_plugin_global_shortcut::ShortcutState;

use base64::Engine;
use pinleaf_core::{
    new_id, Memoboard, MemoboardType, Note, NoteObject, Reminder, RepeatType, SqliteStore,
    StoragePort, StoreResult, TitleType, ViewType,
};

/// Collapsed (rolled-up) window height in CSS px: title bar, plus the title row if present.
fn collapsed_height(note: &Note) -> f64 {
    if note.title.is_some() {
        52.0
    } else {
        28.0
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

struct AppState {
    store: Mutex<SqliteStore>,
    root_board: String,
    trash_board: String,
    cascade: Mutex<i64>, // new-note position cascade counter
}

#[derive(Serialize)]
struct NoteDto {
    id: String,
    title: Option<String>,
    body: String,
    color: u32,
    skin: Option<String>,
    due_ms: Option<i64>,
    repeat: String, // "none" | "daily" | "weekly"
    tags: Vec<String>,
    starred: bool,
    completed: bool,
    stay_on_top: bool,
    transparency: u8,
    rolled: bool,
}

fn repeat_to_str(r: RepeatType) -> &'static str {
    match r {
        RepeatType::None => "none",
        RepeatType::Daily => "daily",
        RepeatType::Weekly => "weekly",
    }
}
fn repeat_from_str(s: &str) -> RepeatType {
    match s {
        "daily" => RepeatType::Daily,
        "weekly" => RepeatType::Weekly,
        _ => RepeatType::None,
    }
}

fn make_reminder(note_id: &str, start_ms: i64, repeat: RepeatType) -> Reminder {
    Reminder {
        note_id: note_id.to_string(),
        start_time: start_ms,
        repeat_type: repeat,
        repeat_every: 1,
        show_reminder: true,
        play_sound: true,
        advance_minutes: 0,
        executed: false,
    }
}

/// Next fire time for a repeating reminder, advanced past `now`; None if it doesn't repeat.
fn next_occurrence(r: &Reminder, now: i64) -> Option<i64> {
    let step = match r.repeat_type {
        RepeatType::Daily => 86_400_000,
        RepeatType::Weekly => 7 * 86_400_000,
        RepeatType::None => return None,
    };
    let every = r.repeat_every.max(1) as i64;
    let mut t = r.start_time;
    while t <= now {
        t += step * every;
    }
    Some(t)
}

// ---- commands (operate on the note identified by the calling window's label) ----

#[tauri::command]
fn load_note(window: WebviewWindow, state: State<AppState>) -> Result<NoteDto, String> {
    let id = window.label().to_string();
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let note = store.get_note(&id).map_err(|e| e.to_string())?;
    let reminder = store.get_reminder(&id).map_err(|e| e.to_string())?;
    let due_ms = reminder.as_ref().map(|r| r.start_time);
    let repeat = reminder
        .as_ref()
        .map(|r| repeat_to_str(r.repeat_type))
        .unwrap_or("none")
        .to_string();
    let tags = store.get_note_tags(&id).map_err(|e| e.to_string())?;
    Ok(NoteDto {
        starred: note.starred,
        completed: note.completed,
        stay_on_top: note.stay_on_top,
        transparency: note.transparency,
        rolled: note.view_type == ViewType::RolledUp,
        id: note.id,
        title: note.title,
        body: note.body,
        color: note.color,
        skin: note.skin,
        due_ms,
        repeat,
        tags,
    })
}

#[tauri::command]
fn save_body(window: WebviewWindow, state: State<AppState>, body: String) -> Result<(), String> {
    let id = window.label().to_string();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
    note.body = body;
    let t = now_ms();
    note.modified_on = t;
    note.modified_content_on = t;
    store.put_note(&note).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_color(window: WebviewWindow, state: State<AppState>, color: u32) -> Result<(), String> {
    let id = window.label().to_string();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
    note.color = color;
    note.modified_on = now_ms();
    store.put_note(&note).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_due(
    window: WebviewWindow,
    state: State<AppState>,
    due: Option<i64>,
    repeat: Option<String>,
) -> Result<(), String> {
    let id = window.label().to_string();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
    match due {
        Some(ms) => {
            let rt = repeat_from_str(repeat.as_deref().unwrap_or("none"));
            store
                .put_reminder(&make_reminder(&id, ms, rt))
                .map_err(|e| e.to_string())?;
            note.reminder_set = true;
        }
        None => {
            store.delete_reminder(&id).map_err(|e| e.to_string())?;
            note.reminder_set = false;
        }
    }
    note.modified_on = now_ms();
    store.put_note(&note).map_err(|e| e.to_string())
}

/// Reschedule the note's reminder `minutes` from now (keeps its repeat setting), re-arming it.
#[tauri::command]
fn snooze(window: WebviewWindow, state: State<AppState>, minutes: i64) -> Result<i64, String> {
    let id = window.label().to_string();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let existing = store.get_reminder(&id).map_err(|e| e.to_string())?;
    let repeat = existing.map(|r| r.repeat_type).unwrap_or(RepeatType::None);
    let when = now_ms() + minutes * 60_000;
    store
        .put_reminder(&make_reminder(&id, when, repeat))
        .map_err(|e| e.to_string())?;
    let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
    note.reminder_set = true;
    note.modified_on = now_ms();
    store.put_note(&note).map_err(|e| e.to_string())?;
    Ok(when)
}

#[tauri::command]
fn set_tags(window: WebviewWindow, state: State<AppState>, tags: Vec<String>) -> Result<(), String> {
    let id = window.label().to_string();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    store.set_note_tags(&id, &tags).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_title(
    app: AppHandle,
    window: WebviewWindow,
    state: State<AppState>,
    title: String,
) -> Result<(), String> {
    let id = window.label().to_string();
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
        let t = title.trim();
        note.title = if t.is_empty() { None } else { Some(t.to_string()) };
        note.title_type = if t.is_empty() { TitleType::Auto } else { TitleType::Custom };
        note.modified_on = now_ms();
        store.put_note(&note).map_err(|e| e.to_string())?;
    }
    refresh_tray_menu(&app); // tray "Open note" labels use the title
    Ok(())
}

#[tauri::command]
fn set_starred(window: WebviewWindow, state: State<AppState>, starred: bool) -> Result<(), String> {
    let id = window.label().to_string();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
    note.starred = starred;
    note.modified_on = now_ms();
    store.put_note(&note).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_completed(window: WebviewWindow, state: State<AppState>, completed: bool) -> Result<(), String> {
    let id = window.label().to_string();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
    note.completed = completed;
    note.modified_on = now_ms();
    store.put_note(&note).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_stay_on_top(window: WebviewWindow, state: State<AppState>, on: bool) -> Result<(), String> {
    let _ = window.set_always_on_top(on);
    let id = window.label().to_string();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
    note.stay_on_top = on;
    note.modified_on = now_ms();
    store.put_note(&note).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_rolled(window: WebviewWindow, state: State<AppState>, rolled: bool) -> Result<(), String> {
    let id = window.label().to_string();
    let (w, h) = {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
        note.view_type = if rolled { ViewType::RolledUp } else { ViewType::Normal };
        note.modified_on = now_ms();
        let w = note.size.0 as f64;
        let h = if rolled {
            collapsed_height(&note)
        } else {
            note.size.1 as f64
        };
        store.put_note(&note).map_err(|e| e.to_string())?;
        (w, h)
    };
    window.set_size(LogicalSize::new(w, h)).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn set_transparency(window: WebviewWindow, state: State<AppState>, value: u8) -> Result<(), String> {
    let id = window.label().to_string();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
    note.transparency = value;
    note.modified_on = now_ms();
    store.put_note(&note).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_skin(window: WebviewWindow, state: State<AppState>, skin: String) -> Result<(), String> {
    let id = window.label().to_string();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
    note.skin = if skin.is_empty() { None } else { Some(skin) };
    note.modified_on = now_ms();
    store.put_note(&note).map_err(|e| e.to_string())
}

#[tauri::command]
fn new_note(app: AppHandle) -> Result<(), String> {
    create_note(&app)
}

/// Store a pasted/dropped image as a NoteObject for the calling note; returns the
/// `pinleaf://object/<id>` reference the frontend inserts into the Markdown body.
#[tauri::command]
fn insert_image(
    window: WebviewWindow,
    state: State<AppState>,
    b64: String,
    mime: String,
) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.as_bytes())
        .map_err(|e| e.to_string())?;
    let id = new_id();
    let obj = NoteObject {
        id: id.clone(),
        note_id: window.label().to_string(),
        mime,
        bytes,
        created_on: now_ms(),
        deleted_on: None,
    };
    state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .put_object(&obj)
        .map_err(|e| e.to_string())?;
    Ok(format!("pinleaf://object/{id}"))
}

fn mime_from_path(p: &str) -> Option<&'static str> {
    match p.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

/// Store an image from a dropped/selected file path as a NoteObject; returns its reference.
#[tauri::command]
fn insert_image_path(
    window: WebviewWindow,
    state: State<AppState>,
    path: String,
) -> Result<String, String> {
    let mime = mime_from_path(&path).ok_or("unsupported image type")?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let id = new_id();
    let obj = NoteObject {
        id: id.clone(),
        note_id: window.label().to_string(),
        mime: mime.to_string(),
        bytes,
        created_on: now_ms(),
        deleted_on: None,
    };
    state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .put_object(&obj)
        .map_err(|e| e.to_string())?;
    Ok(format!("pinleaf://object/{id}"))
}

/// Resolve a stored image object to a `data:` URL for the webview. Avoids the platform-specific
/// custom-scheme URL form while keeping the body's `pinleaf://object/<id>` reference portable.
#[tauri::command]
fn get_image(state: State<AppState>, id: String) -> Result<String, String> {
    let obj = state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .get_object(&id)
        .map_err(|e| e.to_string())?
        .ok_or("image not found")?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&obj.bytes);
    Ok(format!("data:{};base64,{}", obj.mime, b64))
}

/// Open an http(s)/mailto link in the OS default handler (so note links don't navigate the
/// note's own webview). Windows-only launcher for now; a cross-platform opener comes with v2.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    let ok = url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:");
    if !ok {
        return Err("blocked url scheme".into());
    }
    std::process::Command::new("cmd")
        .args(["/C", "start", "", &url])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Close the note's window (the note stays in the store; reopen via tray → Show all notes).
#[tauri::command]
fn close_note(window: WebviewWindow) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

/// Move the note to the Trash board and close its window (recoverable via tray → Trash).
#[tauri::command]
fn delete_note(app: AppHandle, window: WebviewWindow, state: State<AppState>) -> Result<(), String> {
    let id = window.label().to_string();
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let mut note = store.get_note(&id).map_err(|e| e.to_string())?;
        note.memoboard_id = state.trash_board.clone();
        note.modified_on = now_ms();
        store.put_note(&note).map_err(|e| e.to_string())?;
    }
    window.close().map_err(|e| e.to_string())?;
    refresh_tray_menu(&app);
    Ok(())
}

// ---- windows ----

fn save_geometry(state: &AppState, id: &str, x: i32, y: i32, w: i32, h: i32) {
    if let Ok(mut store) = state.store.lock() {
        if let Ok(mut note) = store.get_note(id) {
            note.position = (x, y);
            // Don't let the collapsed height overwrite the saved full size while rolled up.
            if note.view_type != ViewType::RolledUp {
                note.size = (w, h);
            }
            note.modified_on = now_ms();
            let _ = store.put_note(&note);
        }
    }
}

fn attach_geometry_saver(app: &AppHandle, win: &WebviewWindow) {
    let handle = app.clone();
    let w = win.clone();
    let id = win.label().to_string();
    win.on_window_event(move |event| match event {
        WindowEvent::Focused(false) | WindowEvent::CloseRequested { .. } => {
            if let (Ok(pos), Ok(size)) = (w.outer_position(), w.inner_size()) {
                save_geometry(
                    handle.state::<AppState>().inner(),
                    &id,
                    pos.x,
                    pos.y,
                    size.width as i32,
                    size.height as i32,
                );
            }
        }
        _ => {}
    });
}

/// Open (or focus, if already open) a window for a note. Window label = note id.
fn open_note_window(app: &AppHandle, note: &Note) -> tauri::Result<()> {
    if let Some(existing) = app.get_webview_window(&note.id) {
        let _ = existing.set_focus();
        return Ok(());
    }
    let win = WebviewWindowBuilder::new(app, note.id.as_str(), WebviewUrl::App("index.html".into()))
        .title("Pinleaf")
        .decorations(false)
        .transparent(true)
        .resizable(true)
        .shadow(false)
        .visible(false)
        .min_inner_size(150.0, 28.0)
        .inner_size(
            note.size.0 as f64,
            if note.view_type == ViewType::RolledUp {
                collapsed_height(note)
            } else {
                note.size.1 as f64
            },
        )
        .build()?;

    if note.position != (0, 0) {
        let _ = win.set_position(tauri::PhysicalPosition::new(note.position.0, note.position.1));
    } else {
        let _ = win.center();
    }
    if note.stay_on_top {
        let _ = win.set_always_on_top(true);
    }
    let _ = win.show();
    let _ = win.set_focus();
    attach_geometry_saver(app, &win);
    Ok(())
}

fn create_note(app: &AppHandle) -> Result<(), String> {
    let note = {
        let state = app.state::<AppState>();
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let mut n = Note::new(new_id(), state.root_board.clone(), now_ms());
        // Cascade position so new notes don't stack dead-center on each other.
        let idx = {
            let mut c = state.cascade.lock().map_err(|e| e.to_string())?;
            let v = *c;
            *c = (*c + 1) % 8;
            v as i32
        };
        n.position = (90 + idx * 32, 90 + idx * 32);
        store.put_note(&n).map_err(|e| e.to_string())?;
        n
    };
    open_note_window(app, &note).map_err(|e| e.to_string())?;
    refresh_tray_menu(app);
    Ok(())
}

fn show_all(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let trash = state.trash_board.clone();
    let notes = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store.list_notes().map_err(|e| e.to_string())?
    };
    for n in notes.iter().filter(|n| n.memoboard_id != trash) {
        let _ = open_note_window(app, n);
    }
    Ok(())
}

/// Restore a trashed note to the root board and open it.
fn restore_note(app: &AppHandle, id: &str) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        let mut note = store.get_note(id).map_err(|e| e.to_string())?;
        note.memoboard_id = state.root_board.clone();
        note.modified_on = now_ms();
        store.put_note(&note).map_err(|e| e.to_string())?;
    }
    open_note_by_id(app, id)?;
    refresh_tray_menu(app);
    Ok(())
}

/// Permanently delete (tombstone) every note currently in the Trash board.
fn empty_trash(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let trash = state.trash_board.clone();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let ids: Vec<String> = store
        .list_notes()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|n| n.memoboard_id == trash)
        .map(|n| n.id)
        .collect();
    for id in &ids {
        // Purge the note's images too, so permanent delete doesn't leak object blobs.
        if let Ok(objs) = store.list_note_objects(id) {
            for o in objs {
                let _ = store.delete_object(&o.id, now_ms());
            }
        }
        store.soft_delete_note(id, now_ms()).map_err(|e| e.to_string())?;
    }
    drop(store);
    refresh_tray_menu(app);
    Ok(())
}

// ---- store seeding ----

fn ensure_board(store: &mut SqliteStore, bt: MemoboardType, name: &str) -> StoreResult<String> {
    if let Some(b) = store
        .list_memoboards()?
        .into_iter()
        .find(|b| b.board_type == bt)
    {
        return Ok(b.id);
    }
    let t = now_ms();
    let id = new_id();
    store.put_memoboard(&Memoboard {
        id: id.clone(),
        name: name.into(),
        parent_id: None,
        board_type: bt,
        color: 0,
        favorite: bt == MemoboardType::Root,
        created_on: t,
        modified_on: t,
        deleted_on: None,
    })?;
    Ok(id)
}

fn seed_welcome(store: &mut SqliteStore, root: &str) -> StoreResult<Note> {
    let t = now_ms();
    let mut n = Note::new(new_id(), root.to_string(), t);
    n.title = Some("Welcome to Pinleaf".into());
    n.title_type = TitleType::Custom;
    n.body = "This note lives on your **desktop** — formatted with *Markdown*.\n\n- [x] Double-click to edit\n- [ ] Try **bold**, *italic*, ==highlight==, ~~strike~~\n- [ ] Add a [link](https://example.com)\n\n> Tip: press Ctrl+Alt+N for a new note.".into();
    n.reminder_set = true;
    store.put_note(&n)?;
    store.put_reminder(&make_reminder(&n.id, t + 86_400_000, RepeatType::None))?;
    store.set_note_tags(&n.id, &["welcome".to_string()])?;
    Ok(n)
}

fn open_note_by_id(app: &AppHandle, id: &str) -> Result<(), String> {
    let note = {
        let state = app.state::<AppState>();
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store.get_note(id).map_err(|e| e.to_string())?
    };
    open_note_window(app, &note).map_err(|e| e.to_string())
}

/// A short tray label for a note: its title, else its first non-empty body line, else "Untitled".
fn note_menu_label(n: &Note) -> String {
    let raw = n
        .title
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| n.body.lines().find(|l| !l.trim().is_empty()))
        .unwrap_or("Untitled")
        .trim();
    if raw.chars().count() > 28 {
        format!("{}…", raw.chars().take(28).collect::<String>())
    } else {
        raw.to_string()
    }
}

/// Build the tray menu fresh, including an "Open note" submenu of the current live notes.
fn build_tray_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let trash_id = app.state::<AppState>().trash_board.clone();

    let new_i = MenuItem::with_id(app, "new", "New note", true, Some("CmdOrCtrl+Alt+N"))?;
    let show_i = MenuItem::with_id(app, "showall", "Show all notes", true, None::<&str>)?;
    let update_i = MenuItem::with_id(app, "update", "Check for updates", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit Pinleaf", true, None::<&str>)?;

    let notes = app
        .state::<AppState>()
        .store
        .lock()
        .map(|s| s.list_notes().unwrap_or_default())
        .unwrap_or_default();
    let (trashed, live): (Vec<Note>, Vec<Note>) =
        notes.into_iter().partition(|n| n.memoboard_id == trash_id);

    // Open-note submenu (live notes).
    let open_items: Vec<MenuItem<Wry>> = live
        .iter()
        .map(|n| MenuItem::with_id(app, format!("open:{}", n.id), note_menu_label(n), true, None::<&str>))
        .collect::<tauri::Result<_>>()?;
    let open_refs: Vec<&dyn IsMenuItem<Wry>> =
        open_items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    let open_sub = Submenu::with_items(app, "Open note", !open_items.is_empty(), &open_refs)?;

    // Trash submenu: restore items + Empty trash.
    let restore_items: Vec<MenuItem<Wry>> = trashed
        .iter()
        .map(|n| MenuItem::with_id(app, format!("restore:{}", n.id), note_menu_label(n), true, None::<&str>))
        .collect::<tauri::Result<_>>()?;
    let sep_t = PredefinedMenuItem::separator(app)?;
    let empty_i = MenuItem::with_id(app, "emptytrash", "Empty trash", !trashed.is_empty(), None::<&str>)?;
    let mut trash_refs: Vec<&dyn IsMenuItem<Wry>> =
        restore_items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    trash_refs.push(&sep_t);
    trash_refs.push(&empty_i);
    let trash_sub = Submenu::with_items(
        app,
        format!("Trash ({})", trashed.len()),
        !trashed.is_empty(),
        &trash_refs,
    )?;

    Menu::with_items(
        app,
        &[
            &new_i,
            &open_sub,
            &show_i,
            &PredefinedMenuItem::separator(app)?,
            &trash_sub,
            &PredefinedMenuItem::separator(app)?,
            &update_i,
            &quit_i,
        ],
    )
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_tray_menu(app)?;
    let _tray = TrayIconBuilder::with_id("pinleaf-tray")
        .icon(app.default_window_icon().cloned().expect("bundled icon"))
        .tooltip("Pinleaf")
        .menu(&menu)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if let Some(note_id) = id.strip_prefix("open:") {
                let _ = open_note_by_id(app, note_id);
            } else if let Some(note_id) = id.strip_prefix("restore:") {
                let _ = restore_note(app, note_id);
            } else {
                match id {
                    "new" => {
                        let _ = create_note(app);
                    }
                    "showall" => {
                        let _ = show_all(app);
                    }
                    "emptytrash" => {
                        let _ = empty_trash(app);
                    }
                    "update" => run_update(app),
                    "quit" => app.exit(0),
                    _ => {}
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// Post a quick OS notification (best-effort).
fn notify(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Check GitHub releases for a newer version; if found, download, install, and relaunch.
fn run_update(app: &AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let updater = match app.updater() {
            Ok(u) => u,
            Err(e) => return notify(&app, "Pinleaf update", &format!("Updater error: {e}")),
        };
        match updater.check().await {
            Ok(Some(update)) => {
                notify(&app, "Pinleaf", &format!("Updating to {}…", update.version));
                match update.download_and_install(|_, _| {}, || {}).await {
                    Ok(_) => app.restart(),
                    Err(e) => notify(&app, "Pinleaf update failed", &e.to_string()),
                }
            }
            Ok(None) => notify(&app, "Pinleaf", "You're on the latest version."),
            Err(e) => notify(&app, "Pinleaf update", &format!("Check failed: {e}")),
        }
    });
}

/// Rebuild the tray menu to reflect the current set of notes (after create/delete).
fn refresh_tray_menu(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("pinleaf-tray") {
        if let Ok(menu) = build_tray_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

/// One scheduler pass: fire reminders due now — mark them executed, bring the note to the front,
/// and post an OS notification.
fn fire_due_reminders(app: &AppHandle) {
    use tauri_plugin_notification::NotificationExt;

    let due = match app.state::<AppState>().store.lock() {
        Ok(store) => store.due_reminders(now_ms()).unwrap_or_default(),
        Err(_) => return,
    };
    for r in due {
        // Advance a repeating reminder to its next occurrence; otherwise mark it executed. Done
        // first so a slow notification can't double-fire on the next tick.
        if let Ok(mut store) = app.state::<AppState>().store.lock() {
            let mut rr = r.clone();
            match next_occurrence(&r, now_ms()) {
                Some(next) => {
                    rr.start_time = next;
                    rr.executed = false;
                }
                None => rr.executed = true,
            }
            let _ = store.put_reminder(&rr);
        }
        let note = app
            .state::<AppState>()
            .store
            .lock()
            .ok()
            .and_then(|s| s.get_note(&r.note_id).ok());
        if let Some(note) = note {
            let _ = open_note_window(app, &note);
            if let Some(w) = app.get_webview_window(&note.id) {
                let _ = w.set_focus();
            }
            let _ = app
                .notification()
                .builder()
                .title("Pinleaf reminder")
                .body(note_menu_label(&note))
                .show();
        }
    }
}

fn main() {
    let new_note_shortcut = "CmdOrCtrl+Alt+N";

    tauri::Builder::default()
        // Must be the first plugin: a second launch focuses the running instance instead of
        // starting another tray/window set.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some((_, w)) = app.webview_windows().into_iter().next() {
                let _ = w.set_focus();
            } else {
                let _ = show_all(app);
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Serve note images to the webview: <img src="pinleaf://object/<id>">.
        .register_uri_scheme_protocol("pinleaf", |ctx, request| {
            let id = request
                .uri()
                .path()
                .rsplit('/')
                .next()
                .unwrap_or("")
                .to_string();
            let obj = ctx
                .app_handle()
                .state::<AppState>()
                .store
                .lock()
                .ok()
                .and_then(|s| s.get_object(&id).ok().flatten());
            match obj {
                Some(o) => tauri::http::Response::builder()
                    .header("Content-Type", o.mime)
                    .body(std::borrow::Cow::<'static, [u8]>::Owned(o.bytes))
                    .unwrap(),
                None => tauri::http::Response::builder()
                    .status(404)
                    .body(std::borrow::Cow::<'static, [u8]>::Owned(Vec::new()))
                    .unwrap(),
            }
        })
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcut(new_note_shortcut)
                .expect("valid shortcut")
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        let _ = create_note(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("pinleaf.sqlite");
            let mut store = SqliteStore::open(db_path.to_string_lossy().as_ref())?;

            let root = ensure_board(&mut store, MemoboardType::Root, "Notes")?;
            let trash = ensure_board(&mut store, MemoboardType::Trash, "Trash")?;
            let live: Vec<Note> = store
                .list_notes()?
                .into_iter()
                .filter(|n| n.memoboard_id != trash)
                .collect();
            let to_open = if live.is_empty() {
                vec![seed_welcome(&mut store, &root)?]
            } else {
                live
            };

            app.manage(AppState {
                store: Mutex::new(store),
                root_board: root,
                trash_board: trash,
                cascade: Mutex::new(0),
            });

            let handle = app.handle().clone();
            for n in &to_open {
                let _ = open_note_window(&handle, n);
            }

            build_tray(&handle)?;

            // Reminder scheduler: check for due reminders every 20s on a background thread.
            let sched = handle.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(20));
                fire_due_reminders(&sched);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_note, save_body, set_title, set_color, set_due, snooze, set_tags, set_skin,
            set_starred, set_completed, set_stay_on_top, set_transparency, set_rolled,
            new_note, close_note, delete_note, open_url, insert_image, insert_image_path, get_image
        ])
        .build(tauri::generate_context!())
        .expect("error while building Pinleaf")
        .run(|_app, event| {
            // Keep running in the tray when the last note window closes (code: None), but let an
            // explicit quit (tray → Quit calls app.exit(0), code: Some) actually exit.
            if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
