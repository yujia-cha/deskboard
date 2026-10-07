//! 위젯 명령 실행기 — `widget.json` 의 `command` 를 주기적으로 돌려 stdout 을 위젯에 푸시한다.
//!
//! - 인스턴스마다 tokio 태스크 하나. **직렬**이라 한 인스턴스의 실행은 겹치지 않는다.
//! - 전역 `Semaphore(4)` 로 동시에 도는 프로세스 수를 제한한다.
//! - 명령은 **스캔한 manifest 에서만** 온다. 프론트는 명령 문자열을 보내지 않는다.
//! - 설정값은 **환경 변수로만** 넘긴다. 명령 문자열에 끼워 넣지 않는다 (인젝션 방지).
//! - 외부 프로세스는 `Exec` 트레이트 뒤에 둔다 — 테스트는 `FakeExec`.

use super::manifest::{CommandSpec, Parse, Run, Shell};
use serde::Serialize;
use serde_json::Value;
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::{Notify, Semaphore};

pub const STDOUT_CAP: usize = 256 * 1024;
pub const STDERR_CAP: usize = 16 * 1024;
/// 동시에 실행할 수 있는 명령 수 (전 인스턴스 합계).
const MAX_CONCURRENT: usize = 4;
/// 숨겨져 있는 동안 다시 보이는지 확인하는 주기.
const HIDDEN_RECHECK: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutput {
    pub ok: bool,
    pub text: String,
    pub data: Option<Value>,
    pub error: Option<String>,
    pub exit_code: Option<i32>,
    /// unix ms
    pub at: u64,
    /// 실행에 걸린 시간 (ms)
    pub ms: u64,
}

// --- 실행 추상화 ---------------------------------------------------------------------

pub struct ExecRequest {
    pub spec: CommandSpec,
    pub dir: PathBuf,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Done { stdout: String, stderr: String, code: Option<i32> },
    TimedOut,
    Failed(String),
}

pub trait Exec: Send + Sync {
    fn run<'a>(&'a self, req: &'a ExecRequest) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>>;
}

/// 실제 프로세스 실행.
pub struct RealExec;

/// 상한까지만 담고 나머지는 **읽어서 버린다** — 안 읽으면 파이프가 차서 자식이 멈춘다.
async fn read_capped<R: AsyncRead + Unpin>(mut r: R, cap: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match r.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let room = cap.saturating_sub(out.len());
                out.extend_from_slice(&buf[..n.min(room)]);
            }
        }
    }
    out
}

impl Exec for RealExec {
    fn run<'a>(&'a self, req: &'a ExecRequest) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        Box::pin(async move {
            use std::process::Stdio;
            let launch = build_launch(&req.spec, &req.dir);
            let mut cmd = tokio::process::Command::new(&launch.program);
            cmd.args(&launch.args);
            #[cfg(windows)]
            if let Some(raw) = &launch.raw {
                cmd.raw_arg(raw);
            }
            cmd.current_dir(&req.dir)
                .envs(req.env.iter().map(|(k, v)| (k, v)))
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            #[cfg(windows)]
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => return Outcome::Failed(format!("실행 실패: {e}")),
            };
            let (Some(out), Some(err)) = (child.stdout.take(), child.stderr.take()) else {
                return Outcome::Failed("출력 파이프를 열지 못했습니다".into());
            };
            let work = async {
                let (o, e, st) = tokio::join!(read_capped(out, STDOUT_CAP), read_capped(err, STDERR_CAP), child.wait());
                (o, e, st)
            };
            match tokio::time::timeout(Duration::from_secs(req.spec.timeout), work).await {
                Ok((o, e, st)) => Outcome::Done {
                    stdout: String::from_utf8_lossy(&o).into_owned(),
                    stderr: String::from_utf8_lossy(&e).into_owned(),
                    code: st.ok().and_then(|s| s.code()),
                },
                Err(_) => {
                    let _ = child.start_kill();
                    Outcome::TimedOut
                }
            }
        })
    }
}

// --- 명령 줄 조립 (순수) ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    /// 인자를 따옴표 처리 없이 그대로 붙일 문자열 (cmd 용, Windows 에서만 쓰인다)
    pub raw: Option<String>,
}

fn join_argv(v: &[String]) -> String {
    v.iter()
        .map(|a| if a.chars().any(char::is_whitespace) { format!("\"{a}\"") } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 위젯 폴더 안의 `.ps1` 파일을 가리키면 그 절대 경로.
fn ps1_in_dir(line: &str, dir: &Path) -> Option<PathBuf> {
    let s = line.trim().trim_matches(|c| c == '"' || c == '\'');
    if !s.to_ascii_lowercase().ends_with(".ps1") {
        return None;
    }
    let p = Path::new(s);
    let full = if p.is_absolute() { p.to_path_buf() } else { dir.join(p) };
    let full = full.canonicalize().ok()?;
    full.starts_with(dir.canonicalize().ok()?).then_some(full).filter(|f| f.is_file())
}

pub fn build_launch(spec: &CommandSpec, dir: &Path) -> Launch {
    let line = match &spec.run {
        Run::Line(s) => s.clone(),
        Run::Argv(v) => join_argv(v),
    };
    match spec.shell {
        Shell::Powershell | Shell::Pwsh => {
            let program = if spec.shell == Shell::Pwsh { "pwsh.exe" } else { "powershell.exe" };
            let mut args: Vec<String> =
                ["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass"].map(String::from).into();
            match &spec.run {
                Run::Line(s) if ps1_in_dir(s, dir).is_some() => {
                    args.push("-File".into());
                    args.push(ps1_in_dir(s, dir).unwrap_or_default().to_string_lossy().to_string());
                }
                _ => {
                    args.push("-Command".into());
                    args.push(format!("[Console]::OutputEncoding=[Text.Encoding]::UTF8; {line}"));
                }
            }
            Launch { program: program.into(), args, raw: None }
        }
        Shell::Cmd => Launch {
            program: "cmd.exe".into(),
            args: Vec::new(),
            raw: Some(format!("/D /S /C \"chcp 65001>nul & {line}\"")),
        },
        Shell::None => {
            let argv = match &spec.run {
                Run::Argv(v) => v.clone(),
                Run::Line(s) => vec![s.clone()],
            };
            Launch { program: argv[0].clone(), args: argv[1..].to_vec(), raw: None }
        }
    }
}

/// 설정값 → 환경 변수. 스칼라(문자열·숫자·불리언)만 `DESKBOARD_SETTING_<KEY>` 로 낸다.
pub fn build_env(instance_id: &str, dir: &Path, settings: &Value) -> Vec<(String, String)> {
    let mut env = vec![
        ("DESKBOARD_INSTANCE".to_string(), instance_id.to_string()),
        ("DESKBOARD_WIDGET_DIR".to_string(), dir.to_string_lossy().to_string()),
        ("DESKBOARD_SETTINGS".to_string(), settings.to_string()),
    ];
    if let Some(obj) = settings.as_object() {
        for (k, v) in obj {
            let val = match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => continue,
            };
            let key: String = k.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
            env.push((format!("DESKBOARD_SETTING_{key}"), val));
        }
    }
    env
}

// --- 결과 가공 (순수) -------------------------------------------------------------------

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

pub fn finish(outcome: Outcome, parse: Parse, timeout_secs: u64, ms: u64, at: u64) -> CommandOutput {
    let base = CommandOutput { ok: false, text: String::new(), data: None, error: None, exit_code: None, at, ms };
    match outcome {
        Outcome::TimedOut => CommandOutput { error: Some(format!("시간 초과 ({timeout_secs}s)")), ..base },
        Outcome::Failed(e) => CommandOutput { error: Some(e), ..base },
        Outcome::Done { stdout, stderr, code } => {
            let text = stdout.trim_start_matches('\u{feff}').trim_end().to_string();
            if code != Some(0) {
                let err = stderr.trim();
                let error = if err.is_empty() {
                    match code {
                        Some(c) => format!("종료 코드 {c}"),
                        None => "비정상 종료".to_string(),
                    }
                } else {
                    err.to_string()
                };
                return CommandOutput { text, error: Some(error), exit_code: code, ..base };
            }
            let base = CommandOutput { text: text.clone(), exit_code: code, ..base };
            match parse {
                Parse::Text => CommandOutput { ok: true, ..base },
                Parse::Lines => {
                    let lines = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| Value::String(l.into())).collect();
                    CommandOutput { ok: true, data: Some(Value::Array(lines)), ..base }
                }
                Parse::Json => match serde_json::from_str::<Value>(&text) {
                    Ok(v) => CommandOutput { ok: true, data: Some(v), ..base },
                    Err(e) => CommandOutput { error: Some(format!("JSON 파싱 실패: {e}")), ..base },
                },
            }
        }
    }
}

// --- 인스턴스별 태스크 ------------------------------------------------------------------

pub type Emit = Arc<dyn Fn(&str, &CommandOutput) + Send + Sync>;
pub type Visible = Arc<dyn Fn() -> bool + Send + Sync>;

struct Running {
    widget_id: String,
    key: u64,
    spec: CommandSpec,
    dir: PathBuf,
    settings: Value,
    notify: Arc<Notify>,
    last: Arc<Mutex<Option<CommandOutput>>>,
    handle: tauri::async_runtime::JoinHandle<()>,
}

impl Drop for Running {
    fn drop(&mut self) {
        // 태스크가 끊기면 진행 중이던 자식도 kill_on_drop 으로 정리된다.
        self.handle.abort();
    }
}

pub struct Runners {
    exec: Arc<dyn Exec>,
    sem: Arc<Semaphore>,
    emit: Emit,
    visible: Visible,
    /// `interval` 한 단위의 길이. 실제로는 1초, 테스트에서는 짧게.
    unit: Duration,
    map: Mutex<HashMap<String, Running>>,
}

fn key_of(widget_id: &str, spec: &CommandSpec, dir: &Path, settings: &Value) -> u64 {
    let mut h = DefaultHasher::new();
    format!("{widget_id}|{spec:?}|{}|{settings}", dir.display()).hash(&mut h);
    h.finish()
}

struct LoopCtx {
    instance_id: String,
    spec: CommandSpec,
    dir: PathBuf,
    settings: Value,
    exec: Arc<dyn Exec>,
    sem: Arc<Semaphore>,
    emit: Emit,
    visible: Visible,
    unit: Duration,
    notify: Arc<Notify>,
    last: Arc<Mutex<Option<CommandOutput>>>,
}

async fn run_loop(c: LoopCtx) {
    let mut forced = false;
    loop {
        // 숨겨져 있으면 건너뛴다. 다시 보이면 (2초 안에) 곧바로 돈다. run_now 는 숨김과 상관없이 돈다.
        if !forced && c.spec.pause_when_hidden && !(c.visible)() {
            tokio::select! {
                _ = tokio::time::sleep(HIDDEN_RECHECK.min(c.unit * 2)) => {}
                _ = c.notify.notified() => forced = true,
            }
            continue;
        }
        forced = false;

        let started = Instant::now();
        let out = {
            let _permit = c.sem.acquire().await;
            let req = ExecRequest {
                spec: c.spec.clone(),
                dir: c.dir.clone(),
                env: build_env(&c.instance_id, &c.dir, &c.settings),
            };
            let outcome = c.exec.run(&req).await;
            finish(outcome, c.spec.parse, c.spec.timeout, started.elapsed().as_millis() as u64, now_ms())
        };
        if let Ok(mut g) = c.last.lock() {
            *g = Some(out.clone());
        }
        (c.emit)(&c.instance_id, &out);

        if c.spec.interval == 0 {
            c.notify.notified().await;
            forced = true;
        } else {
            tokio::select! {
                _ = tokio::time::sleep(c.unit * c.spec.interval.min(u32::MAX as u64) as u32) => {}
                _ = c.notify.notified() => forced = true,
            }
        }
    }
}

impl Runners {
    pub fn new(exec: Arc<dyn Exec>, emit: Emit, visible: Visible) -> Self {
        Self::with_unit(exec, emit, visible, Duration::from_secs(1))
    }

    pub fn with_unit(exec: Arc<dyn Exec>, emit: Emit, visible: Visible, unit: Duration) -> Self {
        Self { exec, sem: Arc::new(Semaphore::new(MAX_CONCURRENT)), emit, visible, unit, map: Mutex::new(HashMap::new()) }
    }

    /// 같은 (위젯, 명령, 설정) 으로 이미 돌고 있으면 그대로 둔다. 다르면 교체한다.
    pub fn start(&self, instance_id: &str, widget_id: &str, spec: CommandSpec, dir: PathBuf, settings: Value) {
        let key = key_of(widget_id, &spec, &dir, &settings);
        let Ok(mut map) = self.map.lock() else { return };
        if map.get(instance_id).is_some_and(|r| r.key == key) {
            return;
        }
        map.remove(instance_id); // Drop → 이전 태스크 중단
        let notify = Arc::new(Notify::new());
        let last = Arc::new(Mutex::new(None));
        let ctx = LoopCtx {
            instance_id: instance_id.to_string(),
            spec: spec.clone(),
            dir: dir.clone(),
            settings: settings.clone(),
            exec: self.exec.clone(),
            sem: self.sem.clone(),
            emit: self.emit.clone(),
            visible: self.visible.clone(),
            unit: self.unit,
            notify: notify.clone(),
            last: last.clone(),
        };
        let handle = tauri::async_runtime::spawn(run_loop(ctx));
        map.insert(
            instance_id.to_string(),
            Running { widget_id: widget_id.to_string(), key, spec, dir, settings, notify, last, handle },
        );
    }

    pub fn stop(&self, instance_id: &str) {
        if let Ok(mut m) = self.map.lock() {
            m.remove(instance_id);
        }
    }

    pub fn run_now(&self, instance_id: &str) {
        if let Ok(m) = self.map.lock() {
            if let Some(r) = m.get(instance_id) {
                r.notify.notify_one();
            }
        }
    }

    pub fn last(&self, instance_id: &str) -> Option<CommandOutput> {
        let last = self.map.lock().ok()?.get(instance_id)?.last.clone();
        let out = last.lock().ok()?.clone();
        out
    }

    /// 재스캔 뒤: 위젯이 사라졌거나 명령이 없어졌으면 멈추고, 명령·폴더가 바뀌었으면 새 규격으로 다시 시작한다.
    pub fn reconcile(&self, lookup: &dyn Fn(&str) -> Option<(CommandSpec, PathBuf)>) {
        let snapshot: Vec<(String, String, CommandSpec, PathBuf, Value)> = match self.map.lock() {
            Ok(m) => m
                .iter()
                .map(|(i, r)| (i.clone(), r.widget_id.clone(), r.spec.clone(), r.dir.clone(), r.settings.clone()))
                .collect(),
            Err(_) => return,
        };
        for (instance, widget, spec, dir, settings) in snapshot {
            match lookup(&widget) {
                None => self.stop(&instance),
                Some((new_spec, new_dir)) if new_spec != spec || new_dir != dir => {
                    self.start(&instance, &widget, new_spec, new_dir, settings);
                }
                Some(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn spec(interval: u64) -> CommandSpec {
        CommandSpec {
            run: Run::Line("x".into()),
            shell: Shell::Powershell,
            interval,
            timeout: 3,
            parse: Parse::Text,
            pause_when_hidden: false,
        }
    }

    #[derive(Default)]
    struct FakeExec {
        calls: AtomicUsize,
        running: AtomicUsize,
        max_running: AtomicUsize,
        delay: Duration,
        timeout: bool,
        envs: Mutex<Vec<Vec<(String, String)>>>,
    }

    impl Exec for FakeExec {
        fn run<'a>(&'a self, req: &'a ExecRequest) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
            Box::pin(async move {
                let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
                let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_running.fetch_max(now, Ordering::SeqCst);
                self.envs.lock().unwrap().push(req.env.clone());
                tokio::time::sleep(self.delay).await;
                self.running.fetch_sub(1, Ordering::SeqCst);
                if self.timeout {
                    Outcome::TimedOut
                } else {
                    Outcome::Done { stdout: format!("run {n}\n"), stderr: String::new(), code: Some(0) }
                }
            })
        }
    }

    struct Harness {
        runners: Runners,
        fake: Arc<FakeExec>,
        emitted: Arc<Mutex<Vec<(String, CommandOutput)>>>,
        visible: Arc<std::sync::atomic::AtomicBool>,
    }

    fn harness(fake: FakeExec) -> Harness {
        let fake = Arc::new(fake);
        let emitted: Arc<Mutex<Vec<(String, CommandOutput)>>> = Arc::default();
        let e2 = emitted.clone();
        let visible = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let v2 = visible.clone();
        let runners = Runners::with_unit(
            fake.clone(),
            Arc::new(move |id, o| e2.lock().unwrap().push((id.to_string(), o.clone()))),
            Arc::new(move || v2.load(Ordering::SeqCst)),
            Duration::from_millis(10),
        );
        Harness { runners, fake, emitted, visible }
    }

    fn calls(h: &Harness) -> usize {
        h.fake.calls.load(Ordering::SeqCst)
    }

    async fn wait_until(mut f: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if f() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    fn block<F: Future>(f: F) -> F::Output {
        tauri::async_runtime::block_on(f)
    }

    #[test]
    fn runs_do_not_overlap() {
        let h = harness(FakeExec { delay: Duration::from_millis(40), ..Default::default() });
        block(async {
            h.runners.start("i1", "w", spec(1), PathBuf::from("."), json!({}));
            assert!(wait_until(|| calls(&h) >= 4).await);
        });
        h.runners.stop("i1");
        assert_eq!(h.fake.max_running.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn interval_zero_runs_once_then_on_run_now() {
        let h = harness(FakeExec::default());
        block(async {
            h.runners.start("i1", "w", spec(0), PathBuf::from("."), json!({}));
            assert!(wait_until(|| calls(&h) == 1).await);
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(calls(&h), 1);
            h.runners.run_now("i1");
            assert!(wait_until(|| calls(&h) == 2).await);
            assert!(wait_until(|| h.runners.last("i1").is_some_and(|o| o.text == "run 2")).await);
        });
        let em = h.emitted.lock().unwrap();
        assert_eq!(em.len(), 2);
        assert_eq!(em[0].0, "i1");
        assert!(em[0].1.ok);
    }

    #[test]
    fn stop_halts_runs() {
        let h = harness(FakeExec::default());
        block(async {
            h.runners.start("i1", "w", spec(1), PathBuf::from("."), json!({}));
            assert!(wait_until(|| calls(&h) >= 2).await);
            h.runners.stop("i1");
            tokio::time::sleep(Duration::from_millis(50)).await;
            let n = calls(&h);
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(calls(&h), n);
        });
        assert!(h.runners.last("i1").is_none());
    }

    #[test]
    fn start_is_idempotent_and_replaces_on_change() {
        let h = harness(FakeExec::default());
        block(async {
            h.runners.start("i1", "w", spec(0), PathBuf::from("."), json!({"a": 1}));
            assert!(wait_until(|| calls(&h) == 1).await);
            // 같은 인자 → 그대로 (재실행 없음)
            h.runners.start("i1", "w", spec(0), PathBuf::from("."), json!({"a": 1}));
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(calls(&h), 1);
            // 설정이 다르면 교체 → 새로 한 번 돈다
            h.runners.start("i1", "w", spec(0), PathBuf::from("."), json!({"a": 2}));
            assert!(wait_until(|| calls(&h) == 2).await);
        });
        let envs = h.fake.envs.lock().unwrap();
        assert!(envs[1].contains(&("DESKBOARD_SETTING_A".to_string(), "2".to_string())));
    }

    #[test]
    fn timeout_is_reported() {
        let h = harness(FakeExec { timeout: true, ..Default::default() });
        block(async {
            h.runners.start("i1", "w", spec(0), PathBuf::from("."), json!({}));
            assert!(wait_until(|| h.runners.last("i1").is_some()).await);
        });
        let o = h.runners.last("i1").unwrap();
        assert!(!o.ok);
        assert_eq!(o.error.as_deref(), Some("시간 초과 (3s)"));
    }

    #[test]
    fn pause_when_hidden_skips_until_visible() {
        let h = harness(FakeExec::default());
        h.visible.store(false, Ordering::SeqCst);
        let mut s = spec(1);
        s.pause_when_hidden = true;
        block(async {
            h.runners.start("i1", "w", s, PathBuf::from("."), json!({}));
            tokio::time::sleep(Duration::from_millis(150)).await;
            assert_eq!(calls(&h), 0);
            h.visible.store(true, Ordering::SeqCst);
            assert!(wait_until(|| calls(&h) >= 1).await);
        });
    }

    #[test]
    fn reconcile_stops_missing_and_restarts_changed() {
        let h = harness(FakeExec::default());
        block(async {
            h.runners.start("i1", "gone", spec(0), PathBuf::from("."), json!({}));
            h.runners.start("i2", "chg", spec(0), PathBuf::from("."), json!({"k": "v"}));
            h.runners.start("i3", "same", spec(0), PathBuf::from("."), json!({}));
            assert!(wait_until(|| calls(&h) == 3).await);
            h.runners.reconcile(&|w| match w {
                "chg" => Some((spec(5), PathBuf::from("."))),
                "same" => Some((spec(0), PathBuf::from("."))),
                _ => None,
            });
            assert!(wait_until(|| calls(&h) == 4).await); // i2 만 다시 시작
            assert!(h.runners.last("i1").is_none());
            tokio::time::sleep(Duration::from_millis(60)).await;
            assert_eq!(calls(&h), 4);
        });
    }

    #[test]
    fn finish_parses_outputs() {
        let done = |s: &str, st: &str, code| Outcome::Done { stdout: s.into(), stderr: st.into(), code };
        let o = finish(done("hi\r\n\r\n", "", Some(0)), Parse::Text, 10, 5, 1);
        assert!(o.ok && o.text == "hi" && o.data.is_none() && o.exit_code == Some(0));

        let o = finish(done("\u{feff}[{\"a\":1}]", "", Some(0)), Parse::Json, 10, 0, 0);
        assert_eq!(o.data.unwrap()[0]["a"], 1);

        let o = finish(done("not json", "", Some(0)), Parse::Json, 10, 0, 0);
        assert!(!o.ok && o.error.unwrap().starts_with("JSON 파싱 실패"));

        let o = finish(done("a\n\n  b \r\nc", "", Some(0)), Parse::Lines, 10, 0, 0);
        assert_eq!(o.data.unwrap(), json!(["a", "b", "c"]));

        let o = finish(done("partial", " boom \n", Some(2)), Parse::Text, 10, 0, 0);
        assert!(!o.ok && o.error.as_deref() == Some("boom") && o.text == "partial" && o.exit_code == Some(2));
        let o = finish(done("", "", Some(3)), Parse::Text, 10, 0, 0);
        assert_eq!(o.error.as_deref(), Some("종료 코드 3"));

        let o = finish(Outcome::Failed("실행 실패: x".into()), Parse::Text, 10, 0, 0);
        assert!(!o.ok);
    }

    #[test]
    fn env_has_scalars_only() {
        let env = build_env("inst", Path::new("C:/w"), &json!({"city": "서울", "n": 3, "on": true, "list": [1], "obj": {}, "a-b": "x"}));
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("DESKBOARD_INSTANCE"), Some("inst"));
        assert_eq!(get("DESKBOARD_SETTING_CITY"), Some("서울"));
        assert_eq!(get("DESKBOARD_SETTING_N"), Some("3"));
        assert_eq!(get("DESKBOARD_SETTING_ON"), Some("true"));
        assert_eq!(get("DESKBOARD_SETTING_A_B"), Some("x"));
        assert_eq!(get("DESKBOARD_SETTING_LIST"), None);
        assert_eq!(get("DESKBOARD_SETTING_OBJ"), None);
        assert!(get("DESKBOARD_SETTINGS").unwrap().contains("\"city\""));
    }

    #[test]
    fn launch_lines() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = spec(1);
        s.run = Run::Line("Get-Date".into());
        let l = build_launch(&s, dir.path());
        assert_eq!(l.program, "powershell.exe");
        assert!(l.args.contains(&"-NonInteractive".to_string()));
        assert!(l.args.last().unwrap().ends_with("; Get-Date"));
        assert!(l.args.last().unwrap().starts_with("[Console]::OutputEncoding"));

        s.shell = Shell::Pwsh;
        assert_eq!(build_launch(&s, dir.path()).program, "pwsh.exe");

        // 폴더 안 .ps1 은 -File
        std::fs::write(dir.path().join("run.ps1"), "1").unwrap();
        s.run = Run::Line("./run.ps1".into());
        let l = build_launch(&s, dir.path());
        assert!(l.args.contains(&"-File".to_string()) && l.args.last().unwrap().ends_with("run.ps1"));
        // 폴더 밖 경로는 -Command
        s.run = Run::Line("../outside.ps1".into());
        assert!(build_launch(&s, dir.path()).args.contains(&"-Command".to_string()));

        s.shell = Shell::Cmd;
        s.run = Run::Line("echo hi".into());
        let l = build_launch(&s, dir.path());
        assert_eq!(l.program, "cmd.exe");
        assert_eq!(l.raw.as_deref(), Some("/D /S /C \"chcp 65001>nul & echo hi\""));

        s.shell = Shell::None;
        s.run = Run::Argv(vec!["git".into(), "status".into(), "-s".into()]);
        let l = build_launch(&s, dir.path());
        assert_eq!((l.program.as_str(), l.args), ("git", vec!["status".to_string(), "-s".to_string()]));
    }

    #[test]
    fn read_capped_truncates_but_drains() {
        let data = vec![b'a'; 100_000];
        let out = block(read_capped(&data[..], 1000));
        assert_eq!(out.len(), 1000);
    }

    /// cmd 의 내장 `echo` 는 `chcp 65001` 을 무시하고 ANSI 코드 페이지로 내보낸다(실측) — 그래서
    /// 파일 내용을 그대로 흘려보내는 `type` 으로 "UTF-8 바이트가 그대로 한글로 읽히는지"를 본다.
    #[cfg(windows)]
    #[test]
    fn real_cmd_passes_utf8_bytes() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("k.txt"), "한글").unwrap();
        let mut s = spec(0);
        s.shell = Shell::Cmd;
        s.run = Run::Line("type k.txt".into());
        let req = ExecRequest { spec: s.clone(), dir: dir.path().to_path_buf(), env: build_env("t", dir.path(), &json!({})) };
        let o = finish(block(RealExec.run(&req)), s.parse, s.timeout, 0, 0);
        assert!(o.ok, "{o:?}");
        assert_eq!(o.text, "한글");
    }

    #[cfg(windows)]
    #[test]
    fn real_powershell_echo_is_utf8() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = spec(0);
        s.timeout = 20;
        s.run = Run::Line("Write-Output '한글'".into());
        let req = ExecRequest { spec: s.clone(), dir: dir.path().to_path_buf(), env: build_env("t", dir.path(), &json!({})) };
        let o = finish(block(RealExec.run(&req)), s.parse, s.timeout, 0, 0);
        assert!(o.ok, "{o:?}");
        assert_eq!(o.text, "한글");
    }

    #[cfg(windows)]
    #[test]
    fn real_exec_times_out() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = spec(0);
        s.shell = Shell::Cmd;
        s.timeout = 1;
        s.run = Run::Line("ping -n 6 127.0.0.1 >nul".into());
        let req = ExecRequest { spec: s, dir: dir.path().to_path_buf(), env: vec![] };
        assert_eq!(block(RealExec.run(&req)), Outcome::TimedOut);
    }
}
