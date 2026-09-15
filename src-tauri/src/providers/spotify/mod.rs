//! Spotify — 현재 재생 곡 폴링(5초) + 내 플레이리스트 + 재생 제어.
//!
//! 이벤트: `spotify://status` (로그인 상태), `spotify://playback` (재생 상태, 폴링마다).
//! 폴링은 위젯이 `spotify_set_active(true)` 를 보낸 동안만 돈다.

mod api;
mod auth;

use super::Provider;
use api::{Api, ApiError, Playback, Playlist};
use auth::{AuthError, AuthFile, Token};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Clone, Serialize, Default)]
pub struct Status {
    pub client_id: String,
    pub redirect_uri: String,
    pub logged_in: bool,
    pub login_in_progress: bool,
    pub display_name: String,
    pub premium: bool,
    pub error: Option<String>,
}

struct Inner {
    path: PathBuf,
    file: AuthFile,
    status: Status,
    playlists: Vec<Playlist>,
    active_widgets: u32,
    backoff_until: Option<Instant>,
    cancel_login: Arc<AtomicBool>,
    last_playback: Option<Playback>,
}

pub struct SpotifyState {
    inner: Arc<Mutex<Inner>>,
    http: reqwest::Client,
}

pub struct SpotifyProvider;

impl Provider for SpotifyProvider {
    fn id(&self) -> &'static str {
        "spotify"
    }

    fn start(&self, app: AppHandle) {
        let path = auth::auth_path(&app.path().app_data_dir().expect("app data dir"));
        let file = auth::load(&path);
        let status = Status {
            client_id: file.client_id.clone(),
            redirect_uri: file.redirect_uri.clone(),
            logged_in: file.token.is_some(),
            ..Default::default()
        };
        let inner = Arc::new(Mutex::new(Inner {
            path, file, status, playlists: Vec::new(), active_widgets: 0, backoff_until: None,
            cancel_login: Arc::new(AtomicBool::new(false)), last_playback: None,
        }));
        let http = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().expect("reqwest");
        app.manage(SpotifyState { inner: inner.clone(), http: http.clone() });

        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            // 로그인돼 있으면 프로필/플레이리스트 선로딩
            if inner.lock().map(|i| i.file.token.is_some()).unwrap_or(false) {
                let _ = refresh_profile(&app2).await;
            }
            poll_loop(app2).await;
        });
    }
}

fn emit_status(app: &AppHandle) {
    if let Some(st) = app.try_state::<SpotifyState>() {
        if let Ok(i) = st.inner.lock() {
            let _ = app.emit("spotify://status", &i.status);
        }
    }
}

/// 유효한 access token 을 돌려준다 (만료 임박 시 갱신 + 저장).
async fn access_token(app: &AppHandle) -> Result<String, String> {
    let st = app.state::<SpotifyState>();
    let (client_id, token, path) = {
        let i = st.inner.lock().map_err(|_| "state poisoned")?;
        (i.file.client_id.clone(), i.file.token.clone(), i.path.clone())
    };
    let Some(token) = token else { return Err("로그인이 필요합니다".into()) };
    if !token.expires_soon() {
        return Ok(token.access_token);
    }
    match auth::refresh(&st.http, &client_id, &token).await {
        Ok(new) => {
            if let Ok(mut i) = st.inner.lock() {
                i.file.token = Some(new.clone());
                let _ = auth::save(&path, &i.file);
            }
            Ok(new.access_token)
        }
        Err(e) => {
            log::warn!("spotify refresh failed: {e}");
            if let Ok(mut i) = st.inner.lock() {
                i.file.token = None;
                let _ = auth::save(&path, &i.file);
                i.status.logged_in = false;
                i.status.error = Some("세션이 만료되었습니다. 다시 로그인하세요".into());
            }
            emit_status(app);
            Err("세션 만료".into())
        }
    }
}

async fn with_api<T, F>(app: &AppHandle, f: F) -> Result<T, String>
where
    F: for<'a> FnOnce(Api<'a>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, ApiError>> + Send + 'a>>,
{
    let st = app.state::<SpotifyState>();
    let token = access_token(app).await?;
    let http = st.http.clone();
    let api = Api { http: &http, access_token: &token };
    match f(api).await {
        Ok(v) => Ok(v),
        Err(ApiError::RateLimited(s)) => {
            if let Ok(mut i) = st.inner.lock() {
                i.backoff_until = Some(Instant::now() + Duration::from_secs(s));
            }
            Err(ApiError::RateLimited(s).to_string())
        }
        Err(ApiError::Unauthorized) => {
            // 강제 만료 처리 → 다음 호출에서 refresh
            if let Ok(mut i) = st.inner.lock() {
                if let Some(t) = i.file.token.as_mut() { t.expires_at = 0; }
            }
            Err(ApiError::Unauthorized.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

async fn refresh_profile(app: &AppHandle) -> Result<(), String> {
    let (name, premium) = with_api(app, |api| Box::pin(async move { api.me().await })).await?;
    let lists = with_api(app, |api| Box::pin(async move { api.playlists().await })).await.unwrap_or_default();
    let st = app.state::<SpotifyState>();
    if let Ok(mut i) = st.inner.lock() {
        i.status.display_name = name;
        i.status.premium = premium;
        i.status.logged_in = true;
        i.status.error = None;
        i.playlists = lists;
    }
    emit_status(app);
    Ok(())
}

async fn poll_loop(app: AppHandle) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let st = app.state::<SpotifyState>();
        let should_poll = match st.inner.lock() {
            Ok(i) => i.active_widgets > 0 && i.file.token.is_some() && i.backoff_until.map_or(true, |t| Instant::now() >= t),
            Err(_) => false,
        };
        if !should_poll {
            continue;
        }
        match with_api(&app, |api| Box::pin(async move { api.playback().await })).await {
            Ok(p) => {
                if let Ok(mut i) = st.inner.lock() { i.last_playback = p.clone(); }
                let _ = app.emit("spotify://playback", &p);
            }
            Err(e) => log::debug!("spotify poll: {e}"),
        }
    }
}

// --- 커맨드 ---------------------------------------------------------------

#[tauri::command]
pub fn spotify_status(state: State<'_, SpotifyState>) -> Status {
    state.inner.lock().map(|i| i.status.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn spotify_playlists(state: State<'_, SpotifyState>) -> Vec<Playlist> {
    state.inner.lock().map(|i| i.playlists.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn spotify_last_playback(state: State<'_, SpotifyState>) -> Option<Playback> {
    state.inner.lock().ok().and_then(|i| i.last_playback.clone())
}

#[tauri::command]
pub fn spotify_set_active(state: State<'_, SpotifyState>, active: bool) {
    if let Ok(mut i) = state.inner.lock() {
        i.active_widgets = if active { i.active_widgets + 1 } else { i.active_widgets.saturating_sub(1) };
    }
}

/// 브라우저 로그인. 완료/실패는 `spotify://status` 로 알린다.
#[tauri::command]
pub async fn spotify_login(app: AppHandle, state: State<'_, SpotifyState>, client_id: String, redirect_uri: Option<String>) -> Result<(), String> {
    let client_id = client_id.trim().to_string();
    if client_id.is_empty() {
        return Err("Client ID 를 입력하세요".into());
    }
    let redirect_uri = redirect_uri.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| auth::DEFAULT_REDIRECT_URI.into());
    let (host, port) = auth::parse_redirect(&redirect_uri).map_err(|e| e.to_string())?;

    let cancel = Arc::new(AtomicBool::new(false));
    let path = {
        let mut i = state.inner.lock().map_err(|_| "state poisoned")?;
        if i.status.login_in_progress {
            return Err("이미 로그인 진행 중입니다".into());
        }
        i.file.client_id = client_id.clone();
        i.file.redirect_uri = redirect_uri.clone();
        i.status.client_id = client_id.clone();
        i.status.redirect_uri = redirect_uri.clone();
        i.status.login_in_progress = true;
        i.status.error = None;
        i.cancel_login = cancel.clone();
        let _ = auth::save(&i.path, &i.file);
        i.path.clone()
    };
    emit_status(&app);

    let (verifier, challenge) = auth::pkce_pair();
    let st = auth::random_state();
    let url = auth::authorize_url(&client_id, &redirect_uri, &st, &challenge);
    if let Err(e) = tauri_plugin_opener::open_url(&url, None::<&str>) {
        log::warn!("open browser failed: {e}");
    }

    let http = state.http.clone();
    let inner = state.inner.clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let st2 = st.clone();
        let code = tokio::task::spawn_blocking(move || auth::wait_for_callback(&host, port, &st2, cancel))
            .await
            .unwrap_or(Err(AuthError::Invalid("콜백 스레드 오류".into())));
        let result: Result<Token, AuthError> = match code {
            Ok(code) => auth::exchange_code(&http, &client_id, &redirect_uri, &code, &verifier).await,
            Err(e) => Err(e),
        };
        {
            let mut i = match inner.lock() { Ok(i) => i, Err(_) => return };
            i.status.login_in_progress = false;
            match result {
                Ok(t) => {
                    i.file.token = Some(t);
                    i.status.logged_in = true;
                    i.status.error = None;
                    let _ = auth::save(&path, &i.file);
                }
                Err(e) => i.status.error = Some(e.to_string()),
            }
        }
        emit_status(&app2);
        let logged_in = inner.lock().map(|i| i.status.logged_in).unwrap_or(false);
        if logged_in {
            let _ = refresh_profile(&app2).await;
        }
    });
    Ok(())
}

#[tauri::command]
pub fn spotify_cancel_login(state: State<'_, SpotifyState>) {
    if let Ok(i) = state.inner.lock() {
        i.cancel_login.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
pub fn spotify_logout(app: AppHandle, state: State<'_, SpotifyState>) {
    if let Ok(mut i) = state.inner.lock() {
        i.file.token = None;
        let _ = auth::save(&i.path, &i.file);
        i.status.logged_in = false;
        i.status.display_name.clear();
        i.status.premium = false;
        i.playlists.clear();
        i.last_playback = None;
    }
    emit_status(&app);
}

#[tauri::command]
pub async fn spotify_refresh(app: AppHandle) -> Result<(), String> {
    refresh_profile(&app).await
}

async fn run_control(app: &AppHandle, action: String, arg: Option<String>) -> Result<(), String> {
    with_api(app, move |api| Box::pin(async move {
        match action.as_str() {
            "play" => api.play().await,
            "pause" => api.pause().await,
            "next" => api.next().await,
            "previous" => api.previous().await,
            "play_context" => api.play_context(arg.as_deref().unwrap_or("")).await,
            "volume" => api.set_volume(arg.as_deref().and_then(|v| v.parse().ok()).unwrap_or(50)).await,
            "shuffle" => api.set_shuffle(arg.as_deref() == Some("true")).await,
            other => Err(ApiError::Status(400, format!("unknown action {other}"))),
        }
    })).await
}

/// 재생 제어. 활성 디바이스가 없으면 하나 깨워서 한 번 더 시도한다.
#[tauri::command]
pub async fn spotify_control(app: AppHandle, action: String, arg: Option<String>) -> Result<(), String> {
    let is_volume = action == "volume";
    match run_control(&app, action.clone(), arg.clone()).await {
        Err(e) if e == ApiError::NoActiveDevice.to_string() => {
            let woke = with_api(&app, |api| Box::pin(async move { api.wake_device().await })).await.unwrap_or(false);
            if !woke {
                return Err(e);
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
            run_control(&app, action, arg).await
        }
        r => r,
    }?;
    // 제어 직후 상태를 바로 갱신 (볼륨은 드래그 중 연속 호출이라 다음 폴링에 맡긴다)
    if !is_volume {
        if let Ok(p) = with_api(&app, |api| Box::pin(async move { api.playback().await })).await {
            let _ = app.emit("spotify://playback", &p);
        }
    }
    Ok(())
}
