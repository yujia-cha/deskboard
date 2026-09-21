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

    // 자동 업데이트 (GitHub 릴리스의 `latest.json` → 서명된 NSIS 설치본).
    // **release 빌드에서만** 건다 — 개발 실행의 버전은 설치본과 무관해서, 걸어 두면
    // `npm run tauri dev` 가 매번 자기 자신을 업데이트하겠다고 나선다.
    // 프론트(`core/updater.ts`)도 DEV 에서는 확인 자체를 건너뛴다.
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        // 업데이트를 설치한 뒤 앱을 다시 띄우는 데 쓴다 (`process:allow-restart`).
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            window::build_tray(app.handle())?;
            // Win+D(바탕화면 보기)로 대시보드가 같이 숨지 않게 한다.
            window::exclude_from_show_desktop(app.handle());
            window::start_hit_test(app.handle().clone());
            // 전경이 우리에게 잘못 넘어오는 순간을 잡는다 (메시지 루프가 있는 이 스레드에서).
            window::watch_foreground(app.handle());
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
            window::ui_set_text_focus,
            providers::calendar::calendar_list,
            providers::calendar::calendar_upsert,
            providers::calendar::calendar_delete,
            providers::spotify::spotify_status,
            providers::spotify::spotify_playlists,
            providers::spotify::spotify_last_playback,
            providers::spotify::spotify_set_active,
            providers::spotify::spotify_poll,
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
            providers::wallpaper::get_wallpaper,
            providers::wallpaper::wallpaper_set_active,
            providers::sysmon::proc::sysmon_set_detail,
            providers::weather::weather_set_active,
            providers::weather::weather_search,
            providers::notes::notes_list,
            providers::notes::notes_upsert,
            providers::notes::notes_delete,
            providers::notes::notes_clear_done,
            providers::notes::notes_reorder,
            providers::activity::activity_set_active,
            providers::activity::activity_usage,
            providers::activity::activity_sessions,
            providers::activity::activity_set_category,
            providers::github::github_set_token,
            providers::github::github_has_token,
            providers::github::github_logout,
            providers::github::github_set_active,
            providers::github::github_fetch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running deskboard");
}
