//! 데이터 소스 플러그인 레지스트리.
//!
//! 새 위젯 백엔드를 추가하려면:
//! 1. `providers/<name>/mod.rs` 에 `Provider` 를 구현한다.
//!    - 주기적 데이터는 `app.emit("<id>://update", payload)` 로 푸시
//!    - 프론트가 호출하는 조작은 `#[tauri::command]` 로 노출
//! 2. 아래 `all()` 에 인스턴스를 추가한다.
//! 3. 커맨드를 `lib.rs` 의 `generate_handler!` 에 등록한다.

pub mod activity;
pub mod calendar;
pub mod claude_usage;
pub mod folders;
pub mod git;
pub mod github;
pub mod notes;
pub mod oauth;
pub mod secrets;
pub mod spotify;
pub mod sysmon;
pub mod wallpaper;
pub mod weather;

use tauri::AppHandle;
use tauri::Manager;
use std::path::PathBuf;

/// 설정·DB·토큰을 둘 폴더 (`%APPDATA%/com.user.deskboard`).
///
/// **여기서 패닉하면 안 된다.** 로밍 프로파일이 깨졌거나 APPDATA 가 리디렉션된 계정에서
/// `app_data_dir()` 이 실패하는데, 그때 `expect` 하면 대시보드가 통째로 뜨지 않는다.
/// 위젯 하나가 임시 폴더에 기록을 남기는 편이 아무것도 못 쓰는 것보다 낫다.
pub fn data_dir(app: &AppHandle) -> PathBuf {
    match app.path().app_data_dir() {
        Ok(d) => d,
        Err(e) => {
            let tmp = std::env::temp_dir().join("com.user.deskboard");
            log::error!("앱 데이터 폴더를 찾지 못했습니다 ({e}) — {} 로 대체합니다", tmp.display());
            tmp
        }
    }
}

pub trait Provider: Send + Sync {
    /// 이벤트 이름 접두사로 쓰이는 고유 id (예: `sysmon` → `sysmon://update`).
    fn id(&self) -> &'static str;
    /// 앱 시작 시 한 번 호출. 백그라운드 태스크를 띄우고 즉시 반환해야 한다.
    fn start(&self, app: AppHandle);
}

pub fn all() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(sysmon::SysmonProvider),
        Box::new(claude_usage::ClaudeUsageProvider),
        Box::new(calendar::CalendarProvider),
        Box::new(spotify::SpotifyProvider),
        Box::new(folders::FoldersProvider),
        Box::new(wallpaper::WallpaperProvider),
        Box::new(weather::WeatherProvider),
        Box::new(notes::NotesProvider),
        Box::new(activity::ActivityProvider),
        Box::new(git::GitProvider),
        Box::new(github::GithubProvider),
    ]
}
