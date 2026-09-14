//! 데이터 소스 플러그인 레지스트리.
//!
//! 새 위젯 백엔드를 추가하려면:
//! 1. `providers/<name>/mod.rs` 에 `Provider` 를 구현한다.
//!    - 주기적 데이터는 `app.emit("<id>://update", payload)` 로 푸시
//!    - 프론트가 호출하는 조작은 `#[tauri::command]` 로 노출
//! 2. 아래 `all()` 에 인스턴스를 추가한다.
//! 3. 커맨드를 `lib.rs` 의 `generate_handler!` 에 등록한다.

pub mod sysmon;

use tauri::AppHandle;

pub trait Provider: Send + Sync {
    /// 이벤트 이름 접두사로 쓰이는 고유 id (예: `sysmon` → `sysmon://update`).
    fn id(&self) -> &'static str;
    /// 앱 시작 시 한 번 호출. 백그라운드 태스크를 띄우고 즉시 반환해야 한다.
    fn start(&self, app: AppHandle);
}

pub fn all() -> Vec<Box<dyn Provider>> {
    vec![Box::new(sysmon::SysmonProvider)]
}
