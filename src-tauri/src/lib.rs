//! deskboard — Windows 데스크톱 위젯 대시보드 (Tauri 2 백엔드).
//!
//! 구조:
//! - `window`    : 투명/Acrylic 전환, 트레이 메뉴
//! - `providers` : 데이터 소스 플러그인 (`Provider` 트레이트). 새 위젯의 백엔드는 여기에 추가한다.

mod providers;
mod window;

use tauri::Manager;

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let main = app.get_webview_window("main").expect("main window");
            window::apply_theme_mode(&main, window::ThemeMode::Translucent);
            window::build_tray(app.handle())?;

            for provider in providers::all() {
                log::info!("starting provider `{}`", provider.id());
                provider.start(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            window::set_theme_mode,
            window::set_click_through,
        ])
        .run(tauri::generate_context!())
        .expect("error while running deskboard");
}
