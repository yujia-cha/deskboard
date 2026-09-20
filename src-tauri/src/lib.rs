//! deskboard — Windows 데스크톱 위젯 대시보드 (Tauri 2 백엔드).
//!
//! 구조:
//! - `window`    : 투명/Acrylic 전환, 트레이 메뉴
//! - `providers` : 데이터 소스 플러그인 (`Provider` 트레이트). 새 위젯의 백엔드는 여기에 추가한다.

mod autostart;
mod providers;
mod window;

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let builder = tauri::Builder::default();
    // 두 번 실행(자동 시작 + 수동 실행)돼도 대시보드는 하나만 — 기존 창을 보여주고 새 프로세스는 종료.
    // 설치본과 개발 실행이 서로를 가로막지 않도록 release 빌드에서만 건다.
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        use tauri::Manager;
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
        }
    }));

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            window::build_tray(app.handle())?;
            window::start_hit_test(app.handle().clone());
            autostart::sync(app.handle());

            for provider in providers::all() {
                log::info!("starting provider `{}`", provider.id());
                provider.start(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            autostart::autostart_status,
            window::list_monitors,
            window::set_canvas_monitor,
            providers::claude_usage::get_claude_usage,
            providers::claude_usage::get_claude_limits,
            providers::claude_usage::refresh_claude_limits,
            providers::claude_usage::claude_login_start,
            providers::claude_usage::claude_login_finish,
            providers::claude_usage::claude_logout,
            window::set_hit_regions,
            providers::calendar::calendar_list,
            providers::calendar::calendar_upsert,
            providers::calendar::calendar_delete,
            providers::spotify::spotify_status,
            providers::spotify::spotify_playlists,
            providers::spotify::spotify_last_playback,
            providers::spotify::spotify_set_active,
            providers::spotify::spotify_login,
            providers::spotify::spotify_cancel_login,
            providers::spotify::spotify_logout,
            providers::spotify::spotify_refresh,
            providers::spotify::spotify_control,
            providers::folders::folder_ensure_dir,
            providers::folders::folder_list,
            providers::folders::folder_launch,
            providers::folders::folder_reveal,
            providers::folders::folder_add,
            providers::folders::folder_take_out,
            providers::folders::folder_recycle,
            providers::folders::folder_read_image,
            providers::folders::folder_watch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running deskboard");
}
