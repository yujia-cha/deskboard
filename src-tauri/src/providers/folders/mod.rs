//! 폴더 위젯 — 디스코드식 바로가기 폴더. 위젯 인스턴스 1개 = 실제 디렉터리 1개.
//!
//! 이벤트: `folders://changed { dir }` (디렉터리 변경 시, `notify` 디바운스 300ms)

mod fs;
mod icon;

use super::Provider;
use base64::{engine::general_purpose::STANDARD, Engine};
use fs::{display_name, is_direct_child, validate_move_target, FolderFs, RealFs};
use icon::IconCache;
use notify_debouncer_mini::{new_debouncer, notify::RecommendedWatcher, notify::RecursiveMode, DebounceEventResult, Debouncer};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub kind: String, // "dir" | "file"
    pub icon: Option<String>,
}

pub struct FoldersState {
    icons: IconCache,
    watchers: Mutex<HashMap<String, Debouncer<RecommendedWatcher>>>,
}

pub struct FoldersProvider;

impl Provider for FoldersProvider {
    fn id(&self) -> &'static str {
        "folders"
    }
    fn start(&self, app: AppHandle) {
        app.manage(FoldersState { icons: IconCache::default(), watchers: Mutex::new(HashMap::new()) });
    }
}

fn dir_path(dir: &str) -> &Path {
    Path::new(dir)
}

/// 연결 모드에서 쓸 폴더. **없는 폴더를 만들지 않는다.**
///
/// 폴더 위젯은 절대 경로를 저장하므로 다른 PC 로 `settings.json` 을 옮기면
/// `C:\Users\<다른 계정>\...` 같은 남의 경로가 따라온다. 그걸 `create_dir_all` 로 지으면
/// 빈 껍데기가 생기고 사용자는 왜 비었는지 알 수 없다. "기존 폴더 연결" 은 말 그대로
/// **이미 있는 것에 잇는 일**이므로, 없으면 이유를 돌려주고 위젯이 그대로 보여준다.
fn link_dir(dir: Option<&str>, is_dir: &dyn Fn(&Path) -> bool) -> Result<PathBuf, String> {
    let d = dir
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .ok_or_else(|| "연결할 폴더를 고르지 않았습니다".to_string())?;
    let p = Path::new(d);
    if is_dir(p) {
        return Ok(p.to_path_buf());
    }
    Err(format!("폴더를 찾을 수 없습니다: {d}"))
}

/// 위젯이 볼 디렉터리를 정한다.
///
/// `source` 가 두 가지 일을 가른다 — 섞으면 사용자가 자기 파일이 어디로 가는지 알 수 없다:
/// - `"link"` : 사용자가 고른 **기존** 폴더. 있는지만 확인하고 그대로 쓴다.
/// - 그 밖(`"managed"`, 옛 저장값의 `None`): 이 인스턴스만의 전용 폴더를 만들어 쓴다.
#[tauri::command]
pub fn folder_ensure_dir(
    app: AppHandle,
    instance_id: String,
    source: Option<String>,
    dir: Option<String>,
) -> Result<String, String> {
    if source.as_deref() == Some("link") {
        let p = link_dir(dir.as_deref(), &|p: &Path| p.is_dir())?;
        return Ok(p.to_string_lossy().to_string());
    }
    let target = super::data_dir(&app).join("folders").join(&instance_id);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// 목록 조회 + 아이콘 추출(COM, blocking)은 메인 스레드를 막지 않도록 blocking 풀에서 실행한다.
#[tauri::command]
pub async fn folder_list(app: AppHandle, dir: String) -> Result<Vec<Entry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<FoldersState>();
        let raw = RealFs.list(dir_path(&dir)).map_err(|e| e.to_string())?;
        Ok(raw
            .into_iter()
            .map(|e| {
                let icon = state.icons.get(&e.path);
                Entry {
                    name: display_name(&e.name),
                    path: e.path.to_string_lossy().to_string(),
                    kind: if e.is_dir { "dir".into() } else { "file".into() },
                    icon,
                }
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn folder_launch(dir: String, path: String) -> Result<(), String> {
    let target = Path::new(&path);
    if !is_direct_child(dir_path(&dir), target).map_err(|e| e.to_string())? {
        return Err("허용되지 않은 경로입니다".into());
    }
    shell_execute_open(target)
}

#[tauri::command]
pub fn folder_reveal(dir: String, path: String) -> Result<(), String> {
    if !is_direct_child(dir_path(&dir), Path::new(&path)).map_err(|e| e.to_string())? {
        return Err("허용되지 않은 경로입니다".into());
    }
    explorer_select(&path)
}

/// `folder_add` 는 여러 파일을 옮기거나 복사할 수 있어 blocking 풀에서 실행한다.
#[tauri::command]
pub async fn folder_add(dir: String, paths: Vec<String>, mode: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir_p = dir_path(&dir);
        let mut out = Vec::new();
        for p in paths {
            let src = Path::new(&p);
            match validate_move_target(dir_p, src) {
                // 이미 이 폴더의 직계 자식이면 손대지 않고 그대로 둔다.
                Ok(true) => { out.push(src.to_string_lossy().to_string()); continue; }
                Ok(false) => {}
                Err(e) => return Err(e.to_string()),
            }
            let dest = if mode == "copy" { RealFs.copy_into(dir_p, src) } else { RealFs.move_into(dir_p, src) }
                .map_err(|e| e.to_string())?;
            out.push(dest.to_string_lossy().to_string());
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn folder_take_out(dir: String, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = Path::new(&path);
        if !is_direct_child(dir_path(&dir), target).map_err(|e| e.to_string())? {
            return Err("허용되지 않은 경로입니다".into());
        }
        let desktop = dirs::desktop_dir().ok_or("바탕화면 경로를 찾을 수 없습니다")?;
        let dest = RealFs.move_into(&desktop, target).map_err(|e| e.to_string())?;
        Ok(dest.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn folder_recycle(dir: String, path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = Path::new(&path);
        if !is_direct_child(dir_path(&dir), target).map_err(|e| e.to_string())? {
            return Err("허용되지 않은 경로입니다".into());
        }
        recycle_path(target)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn folder_read_image(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let ext = Path::new(&path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let mime = match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "ico" => "image/x-icon",
            _ => "application/octet-stream",
        };
        Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn folder_watch(app: AppHandle, state: tauri::State<'_, FoldersState>, dir: String) -> Result<(), String> {
    let mut watchers = state.watchers.lock().map_err(|_| "잠금 실패".to_string())?;
    if watchers.contains_key(&dir) {
        return Ok(());
    }
    let app2 = app.clone();
    let dir2 = dir.clone();
    let mut debouncer = new_debouncer(Duration::from_millis(300), move |res: DebounceEventResult| {
        if res.is_ok() {
            let _ = app2.emit("folders://changed", serde_json::json!({ "dir": dir2 }));
        }
    })
    .map_err(|e| e.to_string())?;
    debouncer
        .watcher()
        .watch(dir_path(&dir), RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    watchers.insert(dir, debouncer);
    Ok(())
}

// --- Windows 셸 연동 ---------------------------------------------------------------

/// 경로에 공백이 있어도 깨지지 않도록 인자 전체를 직접 조립해서 넘긴다 (`.arg` 는 자동 따옴표 처리를 하지 않음).
#[cfg(target_os = "windows")]
fn explorer_select(path: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("explorer")
        .raw_arg(format!("/select,\"{path}\""))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
#[cfg(not(target_os = "windows"))]
fn explorer_select(_path: &str) -> Result<(), String> {
    Err("Windows 전용 기능입니다".into())
}

#[cfg(target_os = "windows")]
fn shell_execute_open(path: &Path) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb = HSTRING::from("open");
    let file = HSTRING::from(path.as_os_str());
    // SAFETY: 두 HSTRING 은 호출이 끝날 때까지 살아있다.
    let code = unsafe { ShellExecuteW(None, &verb, &file, None, None, SW_SHOWNORMAL) };
    if (code.0 as isize) <= 32 {
        Err(format!("실행 실패 (code {})", code.0 as isize))
    } else {
        Ok(())
    }
}
#[cfg(not(target_os = "windows"))]
fn shell_execute_open(_path: &Path) -> Result<(), String> {
    Err("Windows 전용 기능입니다".into())
}

#[cfg(target_os = "windows")]
fn recycle_path(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FO_DELETE, SHFILEOPSTRUCTW};

    // pFrom 은 이중 널 종료 문자열이어야 한다.
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
    let mut op = SHFILEOPSTRUCTW {
        hwnd: HWND::default(),
        wFunc: FO_DELETE,
        pFrom: windows::core::PCWSTR(wide.as_mut_ptr()),
        pTo: windows::core::PCWSTR::null(),
        fFlags: (FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0) as u16,
        fAnyOperationsAborted: Default::default(),
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: windows::core::PCWSTR::null(),
    };
    // SAFETY: wide 는 호출 동안 유효하고 이중 널로 끝난다. FOF_ALLOWUNDO 로 휴지통 이동만 수행한다(영구 삭제 아님).
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 {
        Err(format!("휴지통으로 보내기 실패 (code {code})"))
    } else {
        Ok(())
    }
}
#[cfg(not(target_os = "windows"))]
fn recycle_path(_path: &Path) -> Result<(), String> {
    Err("Windows 전용 기능입니다".into())
}

#[cfg(test)]
mod dir_tests {
    use super::*;

    /// 이 경로들만 "있는 폴더"로 친다.
    fn fake(existing: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |p: &Path| existing.iter().any(|e| Path::new(e) == p)
    }

    #[test]
    fn an_existing_folder_is_linked_as_is() {
        let f = fake(&[r"D:\shortcuts"]);
        assert_eq!(link_dir(Some(r"D:\shortcuts"), &f), Ok(PathBuf::from(r"D:\shortcuts")));
    }

    #[test]
    fn linking_never_creates_a_folder_even_under_an_existing_parent() {
        // "연결" 은 이미 있는 것에 잇는 일이다 — 새로 만들고 싶으면 전용 폴더 모드를 쓴다.
        let f = fake(&[r"D:\shortcuts"]);
        assert!(link_dir(Some(r"D:\shortcuts\games"), &f).is_err());
    }

    #[test]
    fn a_path_from_another_pc_reports_the_reason() {
        let f = fake(&[r"D:\shortcuts"]);
        let e = link_dir(Some(r"C:\Users\someone-else\Desktop\links"), &f).unwrap_err();
        assert!(e.contains("someone-else"), "경로를 알려 줘야 고칠 수 있다: {e}");
    }

    #[test]
    fn an_empty_link_setting_is_an_error_not_a_silent_default() {
        let f = fake(&[]);
        assert!(link_dir(None, &f).is_err());
        assert!(link_dir(Some(""), &f).is_err());
        assert!(link_dir(Some("   "), &f).is_err());
    }
}
