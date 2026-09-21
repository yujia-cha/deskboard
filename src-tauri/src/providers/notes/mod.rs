//! 메모·할 일. SQLite 저장소 + 커맨드. 변경 시 `notes://changed` 이벤트.

mod store;

use super::Provider;
use std::sync::Mutex;
use store::{Note, NotesSource, SqliteStore};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct NotesState(pub Mutex<Box<dyn NotesSource>>);

pub struct NotesProvider;

impl Provider for NotesProvider {
    fn id(&self) -> &'static str {
        "notes"
    }

    fn start(&self, app: AppHandle) {
        let path = app
            .path()
            .app_data_dir()
            .map(|d| d.join("notes.sqlite"))
            .expect("app data dir");
        let store: Box<dyn NotesSource> = match SqliteStore::open(&path) {
            Ok(s) => Box::new(s),
            Err(e) => {
                log::error!("notes: cannot open {} ({e}); falling back to in-memory", path.display());
                Box::new(SqliteStore::in_memory().expect("in-memory sqlite"))
            }
        };
        app.manage(NotesState(Mutex::new(store)));
    }
}

fn lock<'a>(
    state: &'a State<'_, NotesState>,
) -> Result<std::sync::MutexGuard<'a, Box<dyn NotesSource>>, String> {
    state.0.lock().map_err(|_| "메모 저장소를 열 수 없습니다".to_string())
}

#[tauri::command]
pub fn notes_list(state: State<'_, NotesState>, instance_id: String) -> Result<Vec<Note>, String> {
    lock(&state)?.list(&instance_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn notes_upsert(app: AppHandle, state: State<'_, NotesState>, note: Note) -> Result<Note, String> {
    let saved = lock(&state)?.upsert(note).map_err(|e| e.to_string())?;
    let _ = app.emit("notes://changed", ());
    Ok(saved)
}

#[tauri::command]
pub fn notes_delete(app: AppHandle, state: State<'_, NotesState>, id: String) -> Result<bool, String> {
    let ok = lock(&state)?.delete(&id).map_err(|e| e.to_string())?;
    let _ = app.emit("notes://changed", ());
    Ok(ok)
}

#[tauri::command]
pub fn notes_clear_done(
    app: AppHandle,
    state: State<'_, NotesState>,
    instance_id: String,
) -> Result<usize, String> {
    let n = lock(&state)?.clear_done(&instance_id).map_err(|e| e.to_string())?;
    let _ = app.emit("notes://changed", ());
    Ok(n)
}

#[tauri::command]
pub fn notes_reorder(
    app: AppHandle,
    state: State<'_, NotesState>,
    ids: Vec<String>,
) -> Result<(), String> {
    lock(&state)?.reorder(&ids).map_err(|e| e.to_string())?;
    let _ = app.emit("notes://changed", ());
    Ok(())
}
