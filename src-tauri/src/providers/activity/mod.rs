//! 활동 추적 — 무엇을 하며 시간을 보냈는지.
//!
//! 앞에 떠 있는 창의 **실행 파일 이름만** 몇 초마다 확인해 하루치로 누적한다.
//! 창 제목은 저장하지 않는다 — 문서명·탭 제목이 디스크에 남을 이유가 없다.
//!
//! 위젯 셋(플레이타임·앱 사용시간·일일 회고)이 이 하나를 공유한다. 폴링을 셋으로
//! 나누지 않으려고 provider 를 하나만 둔다.
//!
//! 키보드·마우스 입력이 `IDLE_AFTER` 동안 없으면 자리를 비운 것으로 보고 세지 않는다 —
//! 안 그러면 켜두기만 해도 시간이 쌓인다.
//!
//! ## 놓치지 않기 위해 한 것들
//!
//!  - **확인은 자주, 기록은 드물게.** 전경 창을 읽는 것은 Win32 호출 몇 번(수십 마이크로초)
//!    이라 2초마다 해도 싸다. 비싼 것은 SQL 이므로 메모리에 모았다가 30초마다 한 번 적는다.
//!    예전에는 둘을 묶어 5초였고, 그보다 짧게 쓴 프로그램은 통째로 사라졌다.
//!  - **실제로 흐른 시간을 센다.** 틱마다 고정값을 더하면 절전에서 깨어났을 때나 스레드가
//!    밀렸을 때 기록이 어긋난다. 다만 한 틱에 인정하는 값은 `MAX_TICK` 으로 자른다 —
//!    최대 절전 몇 시간을 사용 시간으로 적을 수는 없다.
//!  - **대시보드를 눌렀다고 구간을 끊지 않는다.** 위젯을 클릭하면 전경이 우리가 되는데,
//!    예전에는 그것을 "자리 비움"과 같게 봐서 세던 구간이 거기서 끊기고 시간도 날아갔다.
//!    지금은 `Foreground::Ours` 로 따로 보고 **아무 일도 하지 않는다**(`transition`).

mod rules;
mod store;

use super::Provider;
use rules::{categorize, is_auto_game_excluded, looks_like_game_path, normalize_exe, Category, Seed};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use store::Store;
use tauri::{AppHandle, Emitter, Manager, State};

pub use store::{SessionRow, UsageRow};

/// 전경 창을 확인하는 주기. 짧을수록 잠깐 쓴 프로그램을 덜 놓친다.
const TICK: Duration = Duration::from_secs(2);
/// 모아 둔 시간을 디스크에 적는 주기. 틱마다 쓰면 SQL 이 초당 여러 번 돈다.
const FLUSH_EVERY: Duration = Duration::from_secs(30);
/// 한 틱에 인정하는 최대 초. 절전에서 깨어나면 경과가 몇 시간일 수 있다.
const MAX_TICK: i64 = 15;
/// 입력이 이만큼 없으면 자리를 비운 것으로 본다.
const IDLE_AFTER: Duration = Duration::from_secs(180);
/// 이 시간 안에 같은 프로그램으로 돌아오면 같은 세션으로 잇는다.
const SESSION_GAP: i64 = 300;
/// 이보다 짧은 구간은 세션으로 기록하지 않는다 (알트탭 한 번까지 남길 필요는 없다).
const MIN_SESSION: i64 = 60;
/// 이보다 오래된 기록은 지운다.
const KEEP_DAYS: i64 = 120;
/// 화면 갱신 알림 주기. 5초마다 알리면 위젯이 그때마다 SQL 을 두 번씩 돌리는데,
/// "오늘 몇 시간" 을 보는 데 그만한 해상도는 필요 없다.
const NOTIFY_EVERY: Duration = Duration::from_secs(30);

/// 위젯이 하나라도 떠 있는가. 없으면 폴링하지 않는다.
static ACTIVE: AtomicBool = AtomicBool::new(false);

pub struct ActivityState {
    pub store: Mutex<Store>,
    /// 기본 분류 규칙 (resources/activity-rules.json)
    pub seed: HashMap<String, Category>,
}

/// ISO-8601 로컬 시각 두 개 사이가 `gap` 초 이내인지.
/// 형식이 이상하면 "이어지지 않는다"로 본다 — 엉뚱하게 합치는 것보다 쪼개지는 편이 낫다.
pub fn within_gap(prev_end: &str, next_start: &str, gap: i64) -> bool {
    let parse = |s: &str| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok();
    match (parse(prev_end), parse(next_start)) {
        (Some(a), Some(b)) => {
            let d = (b - a).num_seconds();
            (0..=gap).contains(&d)
        }
        _ => false,
    }
}

// --- 지금 앞에 있는 프로그램 -------------------------------------------------------

/// 지금 전경에 있는 것. 넷을 구분해야 기록이 어긋나지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Foreground {
    /// 다른 프로그램 (정규화한 실행 파일 이름)
    App(String),
    /// 대시보드 자신 — 위젯을 만지는 중이다. "무엇을 쓰는 시간"이 아니지만 **끊지도 않는다**.
    Ours,
    /// 자리를 비웠다 (입력이 `IDLE_AFTER` 동안 없음)
    Away,
    /// 읽지 못했다 — 권한 없는 프로세스, 잠금 화면, 전환 중. 판단을 미룬다.
    Unknown,
}

#[cfg(target_os = "windows")]
fn foreground_pid() -> Option<u32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    // SAFETY: 두 함수 모두 인자 외 상태를 요구하지 않는다.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return None;
    }
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == 0 {
        return None;
    }
    Some(pid)
}

#[cfg(not(target_os = "windows"))]
fn foreground_pid() -> Option<u32> {
    None
}

/// 마지막 입력 이후 흐른 시간. 알 수 없으면 0 (= 사용 중으로 본다).
#[cfg(target_os = "windows")]
fn idle_for() -> Duration {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: info 는 크기를 채운 유효한 구조체다.
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return Duration::ZERO;
    }
    let now = unsafe { GetTickCount() };
    Duration::from_millis(now.wrapping_sub(info.dwTime) as u64)
}

#[cfg(not(target_os = "windows"))]
fn idle_for() -> Duration {
    Duration::ZERO
}

// --- 폴링 루프 ----------------------------------------------------------------------

fn now_local() -> chrono::DateTime<chrono::Local> {
    chrono::Local::now()
}
fn stamp(t: &chrono::DateTime<chrono::Local>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S").to_string()
}
fn day_of(t: &chrono::DateTime<chrono::Local>) -> String {
    t.format("%Y-%m-%d").to_string()
}

struct Run {
    exe: String,
    started: String,
    last: String,
    seconds: i64,
}

/// 이전 틱과 이번 틱을 견줘 무엇을 할지.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transition {
    /// 세고 있던 구간을 닫는다 (세션으로 기록)
    pub close_previous: bool,
    /// 새 구간을 연다
    pub start_new: bool,
    /// 이번 틱의 시간을 누적할 프로그램. None 이면 아무것도 세지 않는다.
    pub count: Option<String>,
}

/// 전경이 `prev` 에서 `now` 로 바뀌었을 때 무엇을 할지.
///
/// 핵심은 **`Ours`/`Unknown` 에서 아무것도 하지 않는 것**이다. 위젯을 한 번 클릭했다고
/// 세던 구간이 끊기면, 짧게 여러 번 만지는 것만으로 하루가 토막 난다.
pub fn transition(prev: Option<&str>, now: &Foreground) -> Transition {
    match now {
        // 판단을 미룬다 — 세던 것을 닫지도, 새로 열지도, 시간을 더하지도 않는다.
        Foreground::Ours | Foreground::Unknown => Transition::default(),
        // 자리를 비움 — 구간만 닫는다
        Foreground::Away => Transition {
            close_previous: prev.is_some(),
            start_new: false,
            count: None,
        },
        Foreground::App(b) => match prev {
            // 같은 프로그램을 계속 쓰는 중
            Some(a) if a == b => Transition {
                close_previous: false,
                start_new: false,
                count: Some(b.clone()),
            },
            // 다른 프로그램으로 넘어감 — 이전 구간을 닫고 새로 연다
            Some(_) => Transition {
                close_previous: true,
                start_new: true,
                count: Some(b.clone()),
            },
            // 돌아옴
            None => Transition {
                close_previous: false,
                start_new: true,
                count: Some(b.clone()),
            },
        },
    }
}

/// 디스크에 적기 전까지 모아 두는 시간. (날짜, 실행 파일) → 초.
type Pending = HashMap<(String, String), i64>;

/// 모아 둔 시간을 한 번의 잠금 안에서 적는다. 비어 있으면 아무 일도 하지 않는다.
fn flush(app: &AppHandle, pending: &mut Pending) {
    if pending.is_empty() {
        return;
    }
    if let Some(st) = app.try_state::<ActivityState>() {
        if let Ok(mut s) = st.store.lock() {
            for ((day, exe), secs) in pending.iter() {
                let _ = s.add_seconds(day, exe, *secs);
            }
        }
    }
    pending.clear();
    notify(app, false);
}

fn run_loop(app: AppHandle) {
    // 전경 pid → 실행 파일 이름. 같은 창을 계속 보는 동안 재조회를 막는다.
    let mut exe_cache: Option<(u32, String)> = None;
    let mut current: Option<Run> = None;
    let mut last_prune = String::new();
    // 아직 디스크에 적지 않은 시간. FLUSH_EVERY 마다, 그리고 구간이 끝날 때 적는다.
    let mut pending: Pending = HashMap::new();
    let mut last_flush = std::time::Instant::now();
    let mut last_tick = std::time::Instant::now();

    loop {
        std::thread::sleep(TICK);
        // 실제로 흐른 시간을 센다 — 틱마다 고정값을 더하면 스레드가 밀리거나 절전에서
        // 깨어났을 때 기록이 어긋난다. 다만 한 번에 인정하는 값은 잘라 둔다.
        let dt = (last_tick.elapsed().as_secs() as i64).clamp(0, MAX_TICK);
        last_tick = std::time::Instant::now();

        if !ACTIVE.load(Ordering::Relaxed) {
            // 위젯이 없으면 세던 구간을 닫고 모아 둔 것까지 적은 뒤 쉰다
            if let Some(run) = current.take() {
                finish(&app, run);
            }
            flush(&app, &mut pending);
            continue;
        }

        let now = now_local();
        let (fg, fresh_path) = if idle_for() >= IDLE_AFTER {
            (Foreground::Away, None)
        } else {
            current_foreground(&mut exe_cache)
        };
        // 경로를 방금 새로 읽었고 게임 설치 폴더 안이면, 처음 보는 프로그램을 규칙에 넣는다.
        // 경로 자체는 저장하지 않는다 — 판단에만 쓴다.
        if let (Foreground::App(exe), Some(path)) = (&fg, &fresh_path) {
            maybe_auto_tag_game(&app, exe, path);
        }

        let step = transition(current.as_ref().map(|r| r.exe.as_str()), &fg);
        if step.close_previous {
            if let Some(run) = current.take() {
                // 구간을 닫을 때는 총계도 함께 맞춰 둔다 — 위젯이 둘을 나란히 보여 준다.
                flush(&app, &mut pending);
                finish(&app, run);
            }
        }
        if let Some(e) = &step.count {
            *pending.entry((day_of(&now), e.clone())).or_insert(0) += dt;
        }
        if step.start_new {
            let e = step.count.clone().unwrap_or_default();
            current = Some(Run {
                exe: e,
                started: stamp(&now),
                last: stamp(&now),
                seconds: dt,
            });
        } else if step.count.is_some() {
            if let Some(run) = current.as_mut() {
                run.seconds += dt;
                run.last = stamp(&now);
            }
        }

        if last_flush.elapsed() >= FLUSH_EVERY {
            last_flush = std::time::Instant::now();
            flush(&app, &mut pending);
        }

        // 하루에 한 번 오래된 기록 정리
        let today = day_of(&now);
        if last_prune != today {
            last_prune = today.clone();
            flush(&app, &mut pending); // 날짜가 바뀌었다 — 어제 것을 어제 날짜로 적어 둔다
            let cutoff = (now - chrono::Duration::days(KEEP_DAYS)).format("%Y-%m-%d").to_string();
            if let Some(st) = app.try_state::<ActivityState>() {
                if let Ok(mut s) = st.store.lock() {
                    let _ = s.prune(&cutoff);
                }
            }
        }
    }
}

/// pid 하나의 실행 파일 경로. `sysinfo` 를 쓰지 않는 이유: `refresh_processes` 는 pid 를
/// 하나만 지정해도 틱마다 10ms 가까이 걸린다(실측). 여기서 필요한 건 이름 하나뿐이라
/// Win32 로 바로 읽는다 — 호출 세 번, 수십 마이크로초.
#[cfg(target_os = "windows")]
fn exe_of_pid(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: 핸들은 반드시 닫고, 버퍼는 길이를 함께 넘긴다.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None; // 권한이 없는 프로세스(시스템 등)
        }
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        // 0 = PROCESS_NAME_WIN32 (일반 경로 형식)
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 || len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

#[cfg(not(target_os = "windows"))]
fn exe_of_pid(_pid: u32) -> Option<String> {
    None
}

/// 지금 전경에 있는 것.
///
/// 같은 프로그램을 계속 쓰는 동안에는 pid 가 그대로라 실행 파일 경로를 다시 읽지 않는다.
/// 읽지 못한 경우를 `Unknown` 으로 따로 돌려주는 것이 중요하다 — 그걸 "자리 비움"과
/// 같게 보면 잠깐의 실패마다 구간이 끊긴다.
///
/// 두 번째 값은 **캐시 미스로 경로를 방금 새로 읽었을 때만** `Some` — 게임 설치 폴더
/// 판단에 쓰고 그 외에는 버린다(경로 자체는 저장하지 않는다).
fn current_foreground(cache: &mut Option<(u32, String)>) -> (Foreground, Option<String>) {
    let Some(pid) = foreground_pid() else {
        return (Foreground::Unknown, None);
    };
    // 대시보드 자신은 세지 않는다. 바탕화면을 보거나 위젯을 만지면 우리가 전경이 되는데,
    // 그건 "deskboard 를 사용한 시간"이 아니다. 그렇다고 쓰던 프로그램의 구간을 끊지도 않는다.
    if pid == std::process::id() {
        return (Foreground::Ours, None);
    }
    if let Some((cached_pid, name)) = cache {
        if *cached_pid == pid {
            return (Foreground::App(name.clone()), None);
        }
    }
    let Some(path) = exe_of_pid(pid) else {
        return (Foreground::Unknown, None);
    };
    let n = normalize_exe(&path);
    if n.is_empty() {
        return (Foreground::Unknown, None);
    }
    *cache = Some((pid, n.clone()));
    (Foreground::App(n), Some(path))
}

/// 게임 설치 폴더 안에서 처음 보는 실행 파일이면 규칙에 `game` 으로 넣는다.
/// user 규칙과 seed 가 우선이다 — seed 에 이미 있으면(예: `steamwebhelper`) 건드리지 않고,
/// `set_rule_if_absent` 라 사용자가 이미 정한 규칙도 덮어쓰지 않는다. 경로는 판단에만
/// 쓰고 저장하지 않는다.
fn maybe_auto_tag_game(app: &AppHandle, exe: &str, path: &str) {
    if !looks_like_game_path(path) || is_auto_game_excluded(exe) {
        return;
    }
    let Some(st) = app.try_state::<ActivityState>() else {
        return;
    };
    if st.seed.contains_key(exe) {
        return;
    }
    let inserted = st
        .store
        .lock()
        .ok()
        .and_then(|mut s| s.set_rule_if_absent(exe, Category::Game).ok())
        .unwrap_or(false);
    if inserted {
        notify(app, true);
    }
}

/// 마지막으로 화면 갱신을 알린 시각.
static LAST_NOTIFY: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// 위젯에 "다시 읽어라" 고 알린다. 너무 자주 부르지 않는다.
fn notify(app: &AppHandle, force: bool) {
    if !force {
        if let Ok(mut last) = LAST_NOTIFY.lock() {
            if let Some(t) = *last {
                if t.elapsed() < NOTIFY_EVERY {
                    return;
                }
            }
            *last = Some(std::time::Instant::now());
        }
    } else if let Ok(mut last) = LAST_NOTIFY.lock() {
        *last = Some(std::time::Instant::now());
    }
    let _ = app.emit("activity://changed", ());
}

fn finish(app: &AppHandle, run: Run) {
    if run.seconds < MIN_SESSION {
        return;
    }
    if let Some(st) = app.try_state::<ActivityState>() {
        if let Ok(mut s) = st.store.lock() {
            let _ = s.close_session(&run.exe, &run.started, &run.last, SESSION_GAP);
        }
    }
    // 구간이 끝난 건 눈에 보이는 변화라 바로 알린다.
    notify(app, true);
}

pub struct ActivityProvider;

impl Provider for ActivityProvider {
    fn id(&self) -> &'static str {
        "activity"
    }

    fn start(&self, app: AppHandle) {
        let path = super::data_dir(&app).join("activity.sqlite");
        let store = match Store::open(&path) {
            Ok(s) => s,
            Err(e) => {
                log::error!("activity: cannot open {} ({e}); in-memory 로 대체", path.display());
                Store::in_memory().expect("in-memory sqlite")
            }
        };
        let seed = load_seed(&app);
        app.manage(ActivityState { store: Mutex::new(store), seed });

        std::thread::Builder::new()
            .name("activity".into())
            .spawn(move || run_loop(app))
            .expect("spawn activity thread");
    }
}

/// 기본 분류 규칙. `pricing.json` 과 같이 바이너리에 포함한다.
/// 앱 데이터 폴더에 `activity-rules.json` 이 있으면 그걸 대신 쓴다 (규칙을 통째로 바꾸고 싶을 때).
const DEFAULT_RULES: &str = include_str!("../../../resources/activity-rules.json");

fn load_seed(app: &AppHandle) -> HashMap<String, Category> {
    let override_file = app
        .path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("activity-rules.json"))
        .filter(|p| p.is_file())
        .and_then(|p| std::fs::read_to_string(p).ok());

    let text = override_file.as_deref().unwrap_or(DEFAULT_RULES);
    match Seed::from_json(text) {
        Ok(s) => s.to_map(),
        Err(e) => {
            log::warn!("activity: {e} — 기본 규칙으로 되돌립니다");
            Seed::from_json(DEFAULT_RULES).map(|s| s.to_map()).unwrap_or_default()
        }
    }
}

// --- 커맨드 --------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Default)]
pub struct Totals {
    pub game: i64,
    pub work: i64,
    pub other: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Summary {
    pub from: String,
    pub to: String,
    pub rows: Vec<UsageRow>,
    pub totals: Totals,
}

fn classify_rows(st: &ActivityState, raw: Vec<(String, i64)>) -> Result<Summary, String> {
    let user: HashMap<String, Category> = st
        .store
        .lock()
        .map_err(|_| "활동 저장소를 열 수 없습니다".to_string())?
        .user_rules()
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect();

    let mut totals = Totals::default();
    let rows = raw
        .into_iter()
        .map(|(exe, seconds)| {
            let c = categorize(&exe, &user, &st.seed);
            match c {
                Category::Game => totals.game += seconds,
                Category::Work => totals.work += seconds,
                Category::Other => totals.other += seconds,
            }
            UsageRow { exe, category: c.as_str().to_string(), seconds }
        })
        .collect();
    Ok(Summary { rows, totals, ..Default::default() })
}

/// 프론트가 위젯 존재 여부를 알려준다. 하나도 없으면 폴링하지 않는다.
#[tauri::command]
pub fn activity_set_active(active: bool) {
    ACTIVE.store(active, Ordering::Relaxed);
}

/// `from`~`to` (YYYY-MM-DD, 양끝 포함) 의 프로그램별 사용 시간과 분류별 합계.
#[tauri::command]
pub fn activity_usage(
    state: State<'_, ActivityState>,
    from: String,
    to: String,
) -> Result<Summary, String> {
    let raw = state
        .store
        .lock()
        .map_err(|_| "활동 저장소를 열 수 없습니다".to_string())?
        .usage(&from, &to)
        .map_err(|e| e.to_string())?;
    let mut s = classify_rows(&state, raw)?;
    s.from = from;
    s.to = to;
    Ok(s)
}

/// 하루의 사용 구간 목록 (최근 순).
#[tauri::command]
pub fn activity_sessions(
    state: State<'_, ActivityState>,
    day: String,
    limit: Option<i64>,
) -> Result<Vec<SessionRow>, String> {
    let raw = state
        .store
        .lock()
        .map_err(|_| "활동 저장소를 열 수 없습니다".to_string())?
        .sessions(&day, limit.unwrap_or(20).clamp(1, 200))
        .map_err(|e| e.to_string())?;
    let user: HashMap<String, Category> = state
        .store
        .lock()
        .map_err(|_| "활동 저장소를 열 수 없습니다".to_string())?
        .user_rules()
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect();

    Ok(raw
        .into_iter()
        .map(|(exe, started_at, ended_at)| {
            let seconds = seconds_between(&started_at, &ended_at);
            let category = categorize(&exe, &user, &state.seed).as_str().to_string();
            SessionRow { exe, category, started_at, ended_at, seconds }
        })
        .collect())
}

/// 두 ISO 시각 사이의 초. 형식이 이상하면 0.
pub fn seconds_between(a: &str, b: &str) -> i64 {
    let parse = |s: &str| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok();
    match (parse(a), parse(b)) {
        (Some(x), Some(y)) => (y - x).num_seconds().max(0),
        _ => 0,
    }
}

/// 사용자가 프로그램 분류를 바꾼다.
#[tauri::command]
pub fn activity_set_category(
    app: AppHandle,
    state: State<'_, ActivityState>,
    exe: String,
    category: String,
) -> Result<(), String> {
    let exe = normalize_exe(&exe);
    if exe.is_empty() {
        return Err("프로그램 이름이 비어 있습니다".into());
    }
    state
        .store
        .lock()
        .map_err(|_| "활동 저장소를 열 수 없습니다".to_string())?
        .set_rule(&exe, Category::parse(&category))
        .map_err(|e| e.to_string())?;
    notify(&app, true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_break_counts_as_the_same_session() {
        assert!(within_gap("2026-09-21T10:00:00", "2026-09-21T10:02:00", 300));
        assert!(within_gap("2026-09-21T10:00:00", "2026-09-21T10:05:00", 300));
    }

    #[test]
    fn a_long_break_does_not() {
        assert!(!within_gap("2026-09-21T10:00:00", "2026-09-21T10:05:01", 300));
        assert!(!within_gap("2026-09-21T10:00:00", "2026-09-21T12:00:00", 300));
    }

    #[test]
    fn a_start_before_the_previous_end_never_merges() {
        // 시계가 뒤로 갔거나 기록이 꼬인 경우 — 합치면 세션이 엉킨다
        assert!(!within_gap("2026-09-21T10:00:00", "2026-09-21T09:59:00", 300));
    }

    #[test]
    fn unparseable_timestamps_split_rather_than_merge() {
        assert!(!within_gap("nonsense", "2026-09-21T10:00:00", 300));
        assert!(!within_gap("2026-09-21T10:00:00", "", 300));
    }

    #[test]
    fn session_length_is_the_difference_in_seconds() {
        assert_eq!(seconds_between("2026-09-21T10:00:00", "2026-09-21T10:30:00"), 1800);
        assert_eq!(seconds_between("2026-09-21T10:00:00", "2026-09-21T10:00:00"), 0);
    }

    #[test]
    fn a_backwards_or_broken_range_is_zero_not_negative() {
        assert_eq!(seconds_between("2026-09-21T10:30:00", "2026-09-21T10:00:00"), 0);
        assert_eq!(seconds_between("bad", "worse"), 0);
    }

    fn app(x: &str) -> Foreground {
        Foreground::App(x.into())
    }

    #[test]
    fn staying_in_one_program_just_keeps_counting() {
        let t = transition(Some("code"), &app("code"));
        assert_eq!(t.count.as_deref(), Some("code"));
        assert!(!t.close_previous && !t.start_new);
    }

    #[test]
    fn switching_closes_the_old_run_and_opens_a_new_one() {
        let t = transition(Some("code"), &app("eldenring"));
        assert!(t.close_previous && t.start_new);
        // 이번 틱은 새 프로그램 쪽에 붙는다
        assert_eq!(t.count.as_deref(), Some("eldenring"));
    }

    #[test]
    fn going_idle_closes_the_run_and_counts_nothing() {
        let t = transition(Some("code"), &Foreground::Away);
        assert!(t.close_previous);
        assert!(!t.start_new);
        assert_eq!(t.count, None);
    }

    #[test]
    fn coming_back_opens_a_run_without_closing_anything() {
        let t = transition(None, &app("code"));
        assert!(!t.close_previous && t.start_new);
        assert_eq!(t.count.as_deref(), Some("code"));
    }

    #[test]
    fn staying_idle_does_nothing_at_all() {
        assert_eq!(transition(None, &Foreground::Away), Transition::default());
    }

    #[test]
    fn touching_the_dashboard_does_not_break_the_run() {
        // 위젯을 클릭하면 전경이 우리가 된다. 예전에는 이것을 자리 비움과 같게 봐서
        // 세던 구간이 끊기고 시간도 날아갔다 — 짧게 여러 번 만지면 하루가 토막 났다.
        let t = transition(Some("eldenring"), &Foreground::Ours);
        assert_eq!(t, Transition::default());
        assert!(!t.close_previous, "구간을 닫으면 안 된다");
        assert_eq!(t.count, None, "대시보드를 만진 시간은 세지 않는다");
    }

    #[test]
    fn an_unreadable_foreground_waits_instead_of_deciding() {
        // 권한 없는 프로세스·잠금 화면·전환 중. 잠깐의 실패로 구간을 끊지 않는다.
        assert_eq!(transition(Some("code"), &Foreground::Unknown), Transition::default());
        assert_eq!(transition(None, &Foreground::Unknown), Transition::default());
    }

    #[test]
    fn time_is_never_counted_for_two_programs_in_one_tick() {
        // 한 틱은 한 프로그램에만 붙어야 총합이 실제 시간을 넘지 않는다
        for (a, b) in [
            (Some("x"), app("y")),
            (Some("x"), app("x")),
            (None, app("y")),
        ] {
            assert!(transition(a, &b).count.is_some());
        }
        for (a, b) in [
            (Some("x"), Foreground::Away),
            (None, Foreground::Away),
            (Some("x"), Foreground::Ours),
            (Some("x"), Foreground::Unknown),
        ] {
            assert!(transition(a, &b).count.is_none());
        }
    }
}
