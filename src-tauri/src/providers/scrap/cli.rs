//! Claude Code CLI 백엔드 — 구독(Pro/Max)으로 웹 검색을 돌린다. `claude -p` 한 번이 한 번의 스크랩이다.
//!
//! 실행 파일 찾기는 `Fs` 뒤에, 프로세스 실행은 `CliRunner` 뒤에 둔다 — 테스트는 가짜로 갈아 끼운다.

use super::api::{clean_items, output_schema, parse_json_lenient, system_prompt, RunOutput, Settings, Usage};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(300);
const STDOUT_CAP: usize = 4 * 1024 * 1024;
const STDERR_CAP: usize = 64 * 1024;
/// 로그인 토큰을 넘겼는데도 인증에 실패한 경우
const AUTH_EXPIRED: &str = "Claude 로그인이 만료됐습니다 — 🔑 에서 다시 로그인해 주세요";
/// 토큰 없이 돌렸는데 CLI 자체 로그인도 없는 경우
const AUTH_MISSING: &str = "Claude 로그인이 필요합니다 — 🔑 에서 '브라우저로 로그인' 을 눌러 주세요";

// ---- 실행 파일 찾기 ----

/// 실행 파일 탐색에 필요한 파일 시스템·환경 변수 보기.
pub trait Fs {
    fn is_file(&self, p: &Path) -> bool;
    /// 폴더 안의 항목 이름들 (없으면 빈 목록)
    fn list(&self, p: &Path) -> Vec<String>;
    fn env(&self, key: &str) -> Option<String>;
}

pub struct RealFs;

impl Fs for RealFs {
    fn is_file(&self, p: &Path) -> bool {
        p.is_file()
    }
    fn list(&self, p: &Path) -> Vec<String> {
        std::fs::read_dir(p).map(|d| d.flatten().filter_map(|e| e.file_name().into_string().ok()).collect()).unwrap_or_default()
    }
    fn env(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.is_empty())
    }
}

fn semver(name: &str) -> Option<(u64, u64, u64)> {
    let mut it = name.split('.').map(|p| p.parse::<u64>());
    let v = (it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?);
    it.next().is_none().then_some(v)
}

/// Claude 데스크톱 앱이 Claude Code 를 깔아 두는 폴더들.
///
/// 데스크톱 앱은 **MSIX 패키지**라 `%APPDATA%\Claude` 에 쓰는 것이 실제로는
/// `%LOCALAPPDATA%\Packages\Claude_<게시자해시>\LocalCache\Roaming\Claude` 에 저장된다.
/// 패키지 안(Claude 가 띄운 프로세스)에서는 `%APPDATA%` 경로로 보이지만, 우리 앱처럼 밖에서 돌면
/// 그 경로는 **없다** — 실측(2026-10): 밖에서는 패키지 폴더에만 있었다. 둘 다 본다.
fn bundled_bases(fs: &impl Fs) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(appdata) = fs.env("APPDATA") {
        out.push(PathBuf::from(appdata).join("Claude").join("claude-code"));
    }
    if let Some(local) = fs.env("LOCALAPPDATA") {
        let packages = PathBuf::from(local).join("Packages");
        let mut names: Vec<String> = fs.list(&packages).into_iter().filter(|n| n.starts_with("Claude_")).collect();
        names.sort();
        for n in names {
            out.push(packages.join(n).join("LocalCache").join("Roaming").join("Claude").join("claude-code"));
        }
    }
    out
}

/// 찾는 순서: PATH 의 claude.exe → `%USERPROFILE%\.local\bin` → Claude 데스크톱 앱이 깔아 둔
/// `…\claude-code\<버전>\<해시>` (`bundled_bases` 의 모든 곳 중 가장 높은 버전).
pub fn find_exe(fs: &impl Fs) -> Option<PathBuf> {
    if let Some(path) = fs.env("PATH") {
        for dir in std::env::split_paths(&path) {
            let c = dir.join("claude.exe");
            if fs.is_file(&c) {
                return Some(c);
            }
        }
    }
    if let Some(home) = fs.env("USERPROFILE") {
        let c = PathBuf::from(home).join(".local").join("bin").join("claude.exe");
        if fs.is_file(&c) {
            return Some(c);
        }
    }
    let mut versions: Vec<((u64, u64, u64), PathBuf)> = Vec::new();
    for base in bundled_bases(fs) {
        for n in fs.list(&base) {
            if let Some(v) = semver(&n) {
                versions.push((v, base.join(n)));
            }
        }
    }
    versions.sort_by_key(|v| std::cmp::Reverse(v.0));
    for (_, vdir) in versions {
        let mut hashes = fs.list(&vdir);
        hashes.sort();
        for h in hashes {
            let c = vdir.join(h).join("claude.exe");
            if fs.is_file(&c) {
                return Some(c);
            }
        }
    }
    None
}

// ---- 인자·출력 ----

fn model_arg(model: &str) -> &'static str {
    if model == "claude-sonnet-5-5" { "sonnet" } else { "opus" }
}

pub fn build_args(s: &Settings) -> Vec<String> {
    let args = [
        "-p",
        "--output-format",
        "json",
        "--json-schema",
        &output_schema().to_string(),
        "--tools",
        "WebSearch,WebFetch",
        "--allowedTools",
        "WebSearch,WebFetch",
        "--model",
        model_arg(&s.model),
        "--effort",
        &s.effort,
        "--no-session-persistence",
        "--strict-mcp-config",
        "--system-prompt",
        &system_prompt(s, true),
    ];
    args.iter().map(|a| a.to_string()).collect()
}

fn is_auth_failure(msg: &str) -> bool {
    let m = msg.to_lowercase();
    ["authenticate", "oauth", "log in", "login"].iter().any(|k| m.contains(k))
}

/// 표준 출력(JSON 한 덩어리 — 오류든 성공이든)을 해석한다. JSON 이 아니면 `None`.
pub fn parse_output(stdout: &str, count: usize, with_token: bool) -> Option<Result<RunOutput, String>> {
    let v: Value = serde_json::from_str(stdout.trim()).ok().filter(Value::is_object)?;
    let result_text = v.get("result").and_then(Value::as_str).unwrap_or("");
    if v.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
        return Some(Err(if is_auth_failure(result_text) {
            (if with_token { AUTH_EXPIRED } else { AUTH_MISSING }).to_string()
        } else {
            let m: String = result_text.trim().chars().take(300).collect();
            if m.is_empty() { "Claude Code 오류".into() } else { format!("Claude Code 오류: {m}") }
        }));
    }
    let data = match v.get("structured_output").filter(|o| o.is_object()) {
        Some(o) => Some(o.clone()),
        None => parse_json_lenient(result_text).filter(|d| d.get("items").is_some()),
    };
    let mut usage = Usage::default();
    if let Some(u) = v.get("usage") {
        usage.add(u);
    }
    Some(match data {
        None => Err("응답을 해석하지 못했습니다".into()),
        Some(d) => {
            let items = clean_items(&d, count);
            if items.is_empty() { Err("조건에 맞는 항목을 찾지 못했습니다".into()) } else { Ok(RunOutput { items, usage: Some(usage), note: None }) }
        }
    })
}

// ---- 프로세스 ----

#[derive(Debug, Default)]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

/// CLI 프로세스에 덧붙일 환경 변수 — 로그인 토큰이 있을 때만.
fn extra_env(token: Option<&str>) -> Vec<(&'static str, String)> {
    token.map(|t| vec![("CLAUDE_CODE_OAUTH_TOKEN", t.to_string())]).unwrap_or_default()
}

pub trait CliRunner {
    async fn run(&self, exe: &Path, args: &[String], stdin: &str, cwd: &Path, token: Option<&str>) -> Result<CliOutput, String>;
}

pub struct ProcessRunner;

impl CliRunner for ProcessRunner {
    async fn run(&self, exe: &Path, args: &[String], stdin: &str, cwd: &Path, token: Option<&str>) -> Result<CliOutput, String> {
        use std::process::Stdio;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // 빈 폴더에서 돌려, 어떤 CLAUDE.md 도 읽히지 않게 한다.
        std::fs::create_dir_all(cwd).map_err(|e| format!("작업 폴더를 만들지 못했습니다: {e}"))?;
        let mut cmd = tokio::process::Command::new(exe);
        cmd.args(args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        cmd.envs(extra_env(token));
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd.spawn().map_err(|e| format!("Claude Code 를 실행하지 못했습니다: {e}"))?;
        let mut sin = child.stdin.take().ok_or("표준 입력을 열지 못했습니다")?;
        let sout = child.stdout.take().ok_or("표준 출력을 열지 못했습니다")?;
        let serr = child.stderr.take().ok_or("표준 오류를 열지 못했습니다")?;
        let input = stdin.to_string();

        let work = async {
            let write = async move {
                let _ = sin.write_all(input.as_bytes()).await;
                drop(sin);
            };
            let read_out = async move {
                let mut b = Vec::new();
                let _ = sout.take(STDOUT_CAP as u64 + 1).read_to_end(&mut b).await;
                b
            };
            let read_err = async move {
                let mut b = Vec::new();
                let _ = serr.take(STDERR_CAP as u64).read_to_end(&mut b).await;
                b
            };
            let (_, out, err) = tokio::join!(write, read_out, read_err);
            if out.len() > STDOUT_CAP {
                let _ = child.kill().await;
                return Err("출력이 너무 커서 중단했습니다".to_string());
            }
            let status = child.wait().await.map_err(|e| e.to_string())?;
            Ok(CliOutput {
                stdout: String::from_utf8_lossy(&out).into_owned(),
                stderr: String::from_utf8_lossy(&err).into_owned(),
                code: status.code(),
            })
        };
        // 시간이 지나 future 가 버려지면 kill_on_drop 이 프로세스를 끝낸다.
        tokio::time::timeout(TIMEOUT, work).await.map_err(|_| "Claude Code 응답이 5분을 넘겨 중단했습니다".to_string())?
    }
}

pub async fn run<R: CliRunner>(runner: &R, exe: &Path, cwd: &Path, s: &Settings, token: Option<&str>) -> Result<RunOutput, String> {
    let o = runner.run(exe, &build_args(s), &s.prompt, cwd, token).await?;
    if let Some(r) = parse_output(&o.stdout, s.count, token.is_some()) {
        return r;
    }
    if o.code != Some(0) {
        let e: String = o.stderr.trim().chars().take(300).collect();
        let code = o.code.map(|c| format!(" (종료 코드 {c})")).unwrap_or_default();
        return Err(if e.is_empty() { format!("Claude Code 가 실패했습니다{code}") } else { format!("Claude Code 가 실패했습니다{code}: {e}") });
    }
    Err("Claude Code 응답이 JSON 이 아닙니다".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct FakeFs {
        files: HashSet<PathBuf>,
        env: HashMap<&'static str, String>,
    }
    impl FakeFs {
        fn with(files: &[&str], env: &[(&'static str, &str)]) -> Self {
            FakeFs { files: files.iter().map(PathBuf::from).collect(), env: env.iter().map(|(k, v)| (*k, v.to_string())).collect() }
        }
    }
    impl Fs for FakeFs {
        fn is_file(&self, p: &Path) -> bool {
            self.files.contains(p)
        }
        fn list(&self, p: &Path) -> Vec<String> {
            let mut out: Vec<String> = self
                .files
                .iter()
                .filter_map(|f| f.strip_prefix(p).ok())
                .filter_map(|rest| rest.components().next())
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.sort();
            out.dedup();
            out
        }
        fn env(&self, key: &str) -> Option<String> {
            self.env.get(key).cloned()
        }
    }

    fn settings(v: Value) -> Settings {
        Settings::from_value(&v)
    }

    #[test]
    fn exe_from_path_first() {
        let fs = FakeFs::with(
            &["C:/tools/claude.exe", "C:/Users/u/.local/bin/claude.exe"],
            &[("PATH", "C:/nope;C:/tools"), ("USERPROFILE", "C:/Users/u")],
        );
        assert_eq!(find_exe(&fs), Some(PathBuf::from("C:/tools/claude.exe")));
    }

    #[test]
    fn exe_from_local_bin_then_bundled() {
        let fs = FakeFs::with(
            &["C:/Users/u/.local/bin/claude.exe", "C:/A/Claude/claude-code/2.1.9/h/claude.exe"],
            &[("PATH", "C:/nope"), ("USERPROFILE", "C:/Users/u"), ("APPDATA", "C:/A")],
        );
        assert_eq!(find_exe(&fs), Some(PathBuf::from("C:/Users/u/.local/bin/claude.exe")));
        let fs = FakeFs::with(&["C:/A/Claude/claude-code/2.1.9/h/claude.exe"], &[("APPDATA", "C:/A")]);
        assert_eq!(find_exe(&fs), Some(PathBuf::from("C:/A/Claude/claude-code/2.1.9/h/claude.exe")));
    }

    #[test]
    fn exe_picks_highest_semver_with_exe() {
        let fs = FakeFs::with(
            &[
                "C:/A/Claude/claude-code/2.1.9/aaa/claude.exe",
                "C:/A/Claude/claude-code/2.1.286/bbb/claude.exe",
                "C:/A/Claude/claude-code/2.1.287/ccc/readme.txt",
                "C:/A/Claude/claude-code/latest/ddd/claude.exe",
            ],
            &[("APPDATA", "C:/A")],
        );
        // 2.1.287 에는 claude.exe 가 없고, 2.1.286 > 2.1.9 (문자열이 아니라 숫자로 비교)
        assert_eq!(find_exe(&fs), Some(PathBuf::from("C:/A/Claude/claude-code/2.1.286/bbb/claude.exe")));
        assert_eq!(find_exe(&FakeFs::default()), None);
    }

    #[test]
    fn exe_in_msix_package_folder() {
        // 우리 앱(패키지 밖)에서는 %APPDATA%\Claude 가 없고 패키지 LocalCache 에만 있다
        let pkg = "C:/L/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude/claude-code";
        let fs = FakeFs::with(
            &[&format!("{pkg}/2.1.284/a/claude.exe"), &format!("{pkg}/2.1.286/b/claude.exe"), "C:/L/Packages/Other_x/LocalCache/Roaming/Claude/claude-code/9.9.9/z/claude.exe"],
            &[("APPDATA", "C:/A"), ("LOCALAPPDATA", "C:/L")],
        );
        assert_eq!(find_exe(&fs), Some(PathBuf::from(format!("{pkg}/2.1.286/b/claude.exe"))));
        // 두 곳에 다 있으면 더 높은 버전
        let fs = FakeFs::with(
            &["C:/A/Claude/claude-code/2.1.290/h/claude.exe", &format!("{pkg}/2.1.286/b/claude.exe")],
            &[("APPDATA", "C:/A"), ("LOCALAPPDATA", "C:/L")],
        );
        assert_eq!(find_exe(&fs), Some(PathBuf::from("C:/A/Claude/claude-code/2.1.290/h/claude.exe")));
    }

    #[test]
    fn args_are_exact() {
        let s = settings(json!({ "prompt": "rust", "model": "claude-sonnet-5-5", "effort": "high" }));
        let a = build_args(&s);
        assert_eq!(&a[..4], ["-p", "--output-format", "json", "--json-schema"]);
        assert_eq!(serde_json::from_str::<Value>(&a[4]).unwrap(), output_schema());
        assert_eq!(&a[5..13], ["--tools", "WebSearch,WebFetch", "--allowedTools", "WebSearch,WebFetch", "--model", "sonnet", "--effort", "high"]);
        assert_eq!(&a[13..16], ["--no-session-persistence", "--strict-mcp-config", "--system-prompt"]);
        assert_eq!(a[16], system_prompt(&s, true));
        assert_eq!(a.len(), 17);
        assert_eq!(build_args(&settings(json!({})))[10], "opus");
    }

    const ITEMS: &str = r#"{"items":[{"title":"A","url":"https://a.com/x","source":"A","published":"2026-01-01","summary":"s"},{"title":"dup","url":"https://a.com/x","source":"","published":"","summary":""}]}"#;

    #[test]
    fn parses_structured_output() {
        let out = json!({ "is_error": false, "result": "", "structured_output": serde_json::from_str::<Value>(ITEMS).unwrap(),
            "usage": { "input_tokens": 10, "cache_read_input_tokens": 5, "output_tokens": 7, "server_tool_use": { "web_search_requests": 2 } } });
        let r = parse_output(&out.to_string(), 8, true).unwrap().unwrap();
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.usage, Some(Usage { input_tokens: 15, output_tokens: 7, web_searches: 2 }));
    }

    #[test]
    fn parses_result_string_json() {
        let out = json!({ "is_error": false, "result": format!("```json\n{ITEMS}\n```") });
        let r = parse_output(&format!("{out}\n"), 8, true).unwrap().unwrap();
        assert_eq!(r.items[0].title, "A");
        let none = json!({ "is_error": false, "result": "그냥 글" });
        assert!(parse_output(&none.to_string(), 8, true).unwrap().is_err());
    }

    #[test]
    fn maps_auth_errors_and_others() {
        let auth = json!({ "type": "result", "is_error": true, "result": "Failed to authenticate: OAuth session expired and could not be refreshed" });
        // 토큰을 넘겼다면 만료, 안 넘겼다면 로그인 필요
        assert_eq!(parse_output(&auth.to_string(), 8, true).unwrap().unwrap_err(), AUTH_EXPIRED);
        assert_eq!(parse_output(&auth.to_string(), 8, false).unwrap().unwrap_err(), AUTH_MISSING);
        let login = json!({ "is_error": true, "result": "Not logged in · Please run /login" });
        assert_eq!(parse_output(&login.to_string(), 8, true).unwrap().unwrap_err(), AUTH_EXPIRED);
        assert_eq!(parse_output(&login.to_string(), 8, false).unwrap().unwrap_err(), AUTH_MISSING);
        let other = json!({ "is_error": true, "result": "boom" });
        assert_eq!(parse_output(&other.to_string(), 8, true).unwrap().unwrap_err(), "Claude Code 오류: boom");
    }

    #[test]
    fn non_json_is_none() {
        assert!(parse_output("Segmentation fault", 8, true).is_none());
        assert!(parse_output("[1,2]", 8, true).is_none());
    }

    struct FakeRunner(CliOutput, std::sync::Mutex<Vec<(String, Option<String>)>>);
    impl CliRunner for FakeRunner {
        async fn run(&self, _exe: &Path, _args: &[String], stdin: &str, _cwd: &Path, token: Option<&str>) -> Result<CliOutput, String> {
            self.1.lock().unwrap().push((stdin.to_string(), token.map(str::to_string)));
            Ok(CliOutput { stdout: self.0.stdout.clone(), stderr: self.0.stderr.clone(), code: self.0.code })
        }
    }

    #[tokio::test]
    async fn run_passes_prompt_and_token_and_reports_failures() {
        let s = settings(json!({ "prompt": "rust" }));
        let (c, w) = (Path::new("c"), Path::new("w"));
        let ok = FakeRunner(CliOutput { stdout: json!({ "result": ITEMS }).to_string(), code: Some(0), ..Default::default() }, Default::default());
        assert_eq!(run(&ok, c, w, &s, Some("tok")).await.unwrap().items.len(), 1);
        assert_eq!(ok.1.lock().unwrap()[0], ("rust".to_string(), Some("tok".to_string())));

        let crash = FakeRunner(CliOutput { stdout: "oops".into(), stderr: " bad flag \n".into(), code: Some(2) }, Default::default());
        assert_eq!(run(&crash, c, w, &s, None).await.unwrap_err(), "Claude Code 가 실패했습니다 (종료 코드 2): bad flag");
        let weird = FakeRunner(CliOutput { stdout: "hello".into(), code: Some(0), ..Default::default() }, Default::default());
        assert!(run(&weird, c, w, &s, None).await.unwrap_err().contains("JSON"));
        // 종료 코드가 0 이 아니어도 JSON 오류 본문이 있으면 그 해석을 쓴다
        let body = json!({ "is_error": true, "result": "Failed to authenticate" }).to_string();
        let authfail = FakeRunner(CliOutput { stdout: body, code: Some(1), ..Default::default() }, Default::default());
        assert_eq!(run(&authfail, c, w, &s, None).await.unwrap_err(), AUTH_MISSING);
        assert_eq!(run(&authfail, c, w, &s, Some("t")).await.unwrap_err(), AUTH_EXPIRED);
    }

    #[test]
    fn env_has_token_only_when_given() {
        assert_eq!(extra_env(Some("tok")), vec![("CLAUDE_CODE_OAUTH_TOKEN", "tok".to_string())]);
        assert!(extra_env(None).is_empty());
    }

    /// 진짜 claude.exe 로 한 번 — 로그인이 만료된 PC 에서는 비용 없이 매핑된 안내가 나와야 한다.
    #[tokio::test]
    #[ignore]
    async fn cli_live() {
        let exe = find_exe(&RealFs).expect("claude.exe 를 찾지 못했습니다");
        println!("exe = {}", exe.display());
        let cwd = std::env::temp_dir().join("deskboard-scrap-cli-test");
        let r = run(&ProcessRunner, &exe, &cwd, &settings(json!({ "prompt": "Rust 프로그래밍", "effort": "low" })), None).await;
        println!("result = {r:?}");
        let e = r.expect_err("로그인이 만료된 PC 가 아니라면 이 테스트는 호출 비용이 듭니다");
        assert_eq!(e, AUTH_MISSING);
    }
}
