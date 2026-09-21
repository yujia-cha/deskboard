//! 개인 일정표. SQLite 저장소 + 커맨드. 변경 시 `calendar://changed` 이벤트.

mod store;

use super::Provider;
use std::sync::Mutex;
use store::{CalendarEvent, CalendarSource, SqliteStore};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct CalendarState(pub Mutex<Box<dyn CalendarSource>>);

pub struct CalendarProvider;

impl Provider for CalendarProvider {
    fn id(&self) -> &'static str {
        "calendar"
    }

    fn start(&self, app: AppHandle) {
        let path = super::data_dir(&app).join("calendar.sqlite");
        let store: Box<dyn CalendarSource> = match SqliteStore::open(&path) {
            Ok(s) => Box::new(s),
            Err(e) => {
                log::error!("calendar: cannot open {} ({e}); falling back to in-memory", path.display());
                Box::new(SqliteStore::in_memory().expect("in-memory sqlite"))
            }
        };
        app.manage(CalendarState(Mutex::new(store)));
    }
}

fn lock<'a>(state: &'a State<'_, CalendarState>) -> Result<std::sync::MutexGuard<'a, Box<dyn CalendarSource>>, String> {
    state.0.lock().map_err(|_| "calendar store poisoned".to_string())
}

#[tauri::command]
pub fn calendar_list(state: State<'_, CalendarState>, from: String, to: String) -> Result<Vec<CalendarEvent>, String> {
    lock(&state)?.list(&from, &to).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn calendar_upsert(app: AppHandle, state: State<'_, CalendarState>, event: CalendarEvent) -> Result<CalendarEvent, String> {
    let saved = lock(&state)?.upsert(event).map_err(|e| e.to_string())?;
    let _ = app.emit("calendar://changed", ());
    Ok(saved)
}

#[tauri::command]
pub fn calendar_delete(app: AppHandle, state: State<'_, CalendarState>, id: String) -> Result<bool, String> {
    let ok = lock(&state)?.delete(&id).map_err(|e| e.to_string())?;
    let _ = app.emit("calendar://changed", ());
    Ok(ok)
}
