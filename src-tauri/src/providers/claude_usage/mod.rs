//! Claude Code 로컬 사용량 — 트랜스크립트 JSONL 을 파싱해 토큰/비용을 집계하고,
//! 파일 변경을 감시해 `claude_usage://update` 로 푸시한다.

mod auth;
mod limits;
mod parser;
mod pricing;

use super::Provider;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use parser::{Scanner, UsageRecord};
use pricing::Pricing;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use tokio::sync::Notify;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize, Default)]
pub struct Totals {
    pub requests: u64,
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub cost_usd: f64,
}

impl Totals {
    fn add(&mut self, r: &UsageRecord, pricing: &Pricing) {
        self.requests += 1;
        self.input += r.input;
        self.output += r.output;
        self.cache_write += r.cache_write_5m + r.cache_write_1h;
        self.cache_read += r.cache_read;
        if let Some(p) = pricing.lookup(&r.model) {
            self.cost_usd += pricing::cost_usd(&p, r.input, r.output, r.cache_write_5m, r.cache_write_1h, r.cache_read);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DayTotals {
    pub date: String, // YYYY-MM-DD (로컬)
    #[serde(flatten)]
    pub totals: Totals,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelTotals {
    pub model: String,
    pub priced: bool,
    #[serde(flatten)]
    pub totals: Totals,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct UsageSummary {
    pub today: Totals,
    pub week: Totals,   // 최근 7일 (오늘 포함)
    pub month: Totals,  // 최근 30일
    pub all_time: Totals,
    pub daily: Vec<DayTotals>,       // 최근 30일, 오래된 순
    pub by_model: Vec<ModelTotals>,  // 최근 30일, 비용 내림차순
    pub current_session: Totals,     // 가장 최근 활동 세션
    pub current_session_id: String,
    pub last_activity: Option<DateTime<Utc>>,
    pub unpriced_models: Vec<String>,
    pub transcripts_dir: String,
}

pub struct UsageState(pub Mutex<UsageSummary>);
pub struct LimitsState {
    pub latest: Mutex<limits::Limits>,
    pub refresh: Arc<Notify>,
    pub token_file: PathBuf,
    pub pending: auth::Pending,
    pub http: reqwest::Client,
}

pub fn transcripts_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude").join("projects")
}

fn local_date(t: &DateTime<Utc>) -> NaiveDate {
    t.with_timezone(&Local).date_naive()
}

pub fn summarize(records: &[UsageRecord], pricing: &Pricing, today: NaiveDate, dir: &Path) -> UsageSummary {
    let mut s = UsageSummary { transcripts_dir: dir.display().to_string(), ..Default::default() };
    let week_start = today - Duration::days(6);
    let month_start = today - Duration::days(29);
    let mut daily: BTreeMap<NaiveDate, Totals> = (0..30).map(|i| (month_start + Duration::days(i), Totals::default())).collect();
    let mut by_model: HashMap<String, Totals> = HashMap::new();
    let mut unpriced: Vec<String> = Vec::new();

    let latest = records.iter().max_by_key(|r| r.timestamp);
    if let Some(l) = latest {
        s.current_session_id = l.session.clone();
        s.last_activity = Some(l.timestamp);
    }

    for r in records {
        let d = local_date(&r.timestamp);
        s.all_time.add(r, pricing);
        if d == today { s.today.add(r, pricing); }
        if d >= week_start { s.week.add(r, pricing); }
        if d >= month_start {
            s.month.add(r, pricing);
            if let Some(t) = daily.get_mut(&d) { t.add(r, pricing); }
            by_model.entry(r.model.clone()).or_default().add(r, pricing);
        }
        if r.session == s.current_session_id { s.current_session.add(r, pricing); }
        if pricing.lookup(&r.model).is_none() && !unpriced.contains(&r.model) {
            unpriced.push(r.model.clone());
        }
    }

    s.daily = daily.into_iter().map(|(d, t)| DayTotals { date: d.format("%Y-%m-%d").to_string(), totals: t }).collect();
    let mut models: Vec<ModelTotals> = by_model
        .into_iter()
        .map(|(m, t)| ModelTotals { priced: pricing.lookup(&m).is_some(), model: m, totals: t })
        .collect();
    models.sort_by(|a, b| b.totals.cost_usd.partial_cmp(&a.totals.cost_usd).unwrap_or(std::cmp::Ordering::Equal)
        .then(b.totals.output.cmp(&a.totals.output)));
    s.by_model = models;
    s.unpriced_models = unpriced;
    s
}

#[tauri::command]
pub fn get_claude_usage(state: tauri::State<'_, UsageState>) -> UsageSummary {
    state.0.lock().map(|s| s.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn get_claude_limits(state: tauri::State<'_, LimitsState>) -> limits::Limits {
    state.latest.lock().map(|s| s.clone()).unwrap_or_default()
}

/// 위젯의 수동 새로고침.
#[tauri::command]
pub fn refresh_claude_limits(state: tauri::State<'_, LimitsState>) {
    state.refresh.notify_one();
}

/// 위젯 로그인 1단계: 인가 URL 을 브라우저로 열고 돌려준다.
#[tauri::command]
pub fn claude_login_start(state: tauri::State<'_, LimitsState>) -> String {
    let url = auth::start(&state.pending);
    if let Err(e) = tauri_plugin_opener::open_url(&url, None::<&str>) {
        log::warn!("open browser failed: {e}");
    }
    url
}

/// 위젯 로그인 2단계: 승인 페이지의 코드를 받아 토큰으로 교환.
#[tauri::command]
pub async fn claude_login_finish(state: tauri::State<'_, LimitsState>, code: String) -> Result<(), String> {
    auth::finish(&state.http, &state.pending, &state.token_file, &code).await?;
    state.refresh.notify_one();
    Ok(())
}

#[tauri::command]
pub fn claude_logout(app: AppHandle, state: tauri::State<'_, LimitsState>) {
    auth::clear(&state.token_file);
    let l = limits::Limits { fetched_at: auth::now_ms(), ..Default::default() };
    if let Ok(mut g) = state.latest.lock() { *g = l.clone(); }
    let _ = app.emit("claude_usage://limits", &l);
}

/// 60초마다, 그리고 트랜스크립트가 바뀔 때마다(디바운스) 한도를 조회해 `claude_usage://limits` 로 푸시.
async fn limits_loop(app: AppHandle, refresh: Arc<Notify>) {
    let (http, path) = {
        let st = app.state::<LimitsState>();
        (st.http.clone(), st.token_file.clone())
    };
    // 조회 간격: 정상 60초. 트랜스크립트 변경/수동 새로고침으로 깨워도 마지막 조회 후 최소 30초는 띄운다.
    // 429 를 받으면 5분 쉰다 (비공식 엔드포인트라 공손하게).
    let mut last_fetch = std::time::Instant::now() - std::time::Duration::from_secs(3600);
    loop {
        let l = limits::fetch(&http, &path).await;
        last_fetch = std::time::Instant::now();
        let rate_limited = l.error.as_deref().map_or(false, |e| e.contains("너무 많습니다"));
        let mut to_emit = l.clone();
        if let Some(st) = app.try_state::<LimitsState>() {
            if let Ok(mut g) = st.latest.lock() {
                // 429 는 직전 정상값을 유지하고 오류 문구만 덧붙인다
                if rate_limited && g.ok { g.error = l.error.clone(); } else { *g = l.clone(); }
                to_emit = g.clone();
            }
        }
        let _ = app.emit("claude_usage://limits", &to_emit);
        let wait = if rate_limited { 300 } else if l.ok { 60 } else if !l.logged_in { 600 } else { 120 };
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(wait)) => {}
            _ = refresh.notified() => {
                let since = last_fetch.elapsed().as_secs();
                let min_gap: u64 = if rate_limited { 120 } else { 30 };
                if since < min_gap {
                    tokio::time::sleep(std::time::Duration::from_secs(min_gap - since)).await;
                } else {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await; // 연속 변경 묶기
                }
            }
        }
    }
}

pub struct ClaudeUsageProvider;

impl Provider for ClaudeUsageProvider {
    fn id(&self) -> &'static str {
        "claude_usage"
    }

    fn start(&self, app: AppHandle) {
        app.manage(UsageState(Mutex::new(UsageSummary::default())));
        let refresh = Arc::new(Notify::new());
        let token_file = auth::token_path(&app.path().app_data_dir().expect("app data dir"));
        app.manage(LimitsState {
            latest: Mutex::new(limits::Limits::default()),
            refresh: refresh.clone(),
            token_file,
            pending: auth::Pending::default(),
            http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().expect("reqwest"),
        });
        tauri::async_runtime::spawn(limits_loop(app.clone(), refresh.clone()));
        std::thread::Builder::new()
            .name("claude_usage".into())
            .spawn(move || run(app, refresh))
            .expect("spawn claude_usage thread");
    }
}

fn publish(app: &AppHandle, scanner: &Scanner, pricing: &Pricing, dir: &Path) {
    let summary = summarize(&scanner.records, pricing, Local::now().date_naive(), dir);
    if let Some(state) = app.try_state::<UsageState>() {
        if let Ok(mut g) = state.0.lock() {
            *g = summary.clone();
        }
    }
    let _ = app.emit("claude_usage://update", &summary);
}

fn run(app: AppHandle, refresh: Arc<Notify>) {
    let dir = transcripts_dir();
    let user_pricing = app
        .path()
        .app_data_dir()
        .map(|d| d.join("pricing.json"))
        .unwrap_or_default();
    let pricing = Pricing::load_with_override(&user_pricing);
    let mut scanner = Scanner::default();

    let n = scanner.scan_dir(&dir);
    log::info!("claude_usage: {n} records from {}", dir.display());
    publish(&app, &scanner, &pricing, &dir);

    if !dir.exists() {
        log::warn!("claude_usage: {} not found; watcher disabled", dir.display());
        return;
    }

    let (tx, rx) = mpsc::channel();
    let mut debouncer = match new_debouncer(std::time::Duration::from_millis(500), tx) {
        Ok(d) => d,
        Err(e) => { log::warn!("claude_usage watcher failed: {e}"); return; }
    };
    if let Err(e) = debouncer.watcher().watch(&dir, RecursiveMode::Recursive) {
        log::warn!("claude_usage watch failed: {e}");
        return;
    }

    // 날짜가 바뀌면 이벤트가 없어도 "오늘" 집계가 바뀌어야 하므로 주기적으로 재발행
    let mut last_day = Local::now().day();
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(Ok(events)) => {
                let mut added = 0;
                for ev in events {
                    if ev.kind != DebouncedEventKind::Any { continue; }
                    if ev.path.extension().and_then(|e| e.to_str()) == Some("jsonl") && ev.path.is_file() {
                        added += scanner.scan_file(&ev.path).unwrap_or(0);
                    }
                }
                if added > 0 {
                    publish(&app, &scanner, &pricing, &dir);
                    refresh.notify_one();
                }
            }
            Ok(Err(e)) => log::debug!("claude_usage watch error: {e}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let day = Local::now().day();
                if day != last_day { last_day = day; publish(&app, &scanner, &pricing, &dir); }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ts: &str, model: &str, session: &str, out: u64) -> UsageRecord {
        UsageRecord {
            timestamp: DateTime::parse_from_rfc3339(ts).unwrap().with_timezone(&Utc),
            model: model.into(), input: 100, output: out, cache_write_5m: 0, cache_write_1h: 0, cache_read: 0,
            key: None, session: session.into(),
        }
    }

    #[test]
    fn buckets_by_day_and_model() {
        let pricing = Pricing::builtin();
        let today = NaiveDate::from_ymd_opt(2026, 9, 14).unwrap();
        // 정오 UTC 는 어떤 로컬 시간대(±12h)에서도 같은 날짜
        let records = vec![
            rec("2026-09-14T12:00:00Z", "claude-opus-5", "s2", 1000),
            rec("2026-09-10T12:00:00Z", "claude-sonnet-5", "s1", 1000),
            rec("2026-08-01T12:00:00Z", "claude-opus-5", "s0", 1000),
            rec("2026-09-14T12:30:00Z", "mystery-model", "s2", 1),
        ];
        let s = summarize(&records, &pricing, today, Path::new("x"));
        assert_eq!(s.today.requests, 2);
        assert_eq!(s.week.requests, 3);
        assert_eq!(s.month.requests, 3);
        assert_eq!(s.all_time.requests, 4);
        assert_eq!(s.daily.len(), 30);
        assert_eq!(s.daily.last().unwrap().date, "2026-09-14");
        assert_eq!(s.current_session_id, "s2");
        assert_eq!(s.current_session.requests, 2);
        assert_eq!(s.by_model[0].model, "claude-opus-5"); // 비용 최다
        assert_eq!(s.unpriced_models, vec!["mystery-model".to_string()]);
        // opus-5: 100 in * 5 + 1000 out * 25 = 0.0005 + 0.025
        assert!((s.today.cost_usd - 0.0255).abs() < 1e-9);
    }
}
