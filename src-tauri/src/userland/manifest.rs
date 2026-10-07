//! `widget.json` 해석. JSONC 를 받아 정규화한 `Manifest` 와 경고 목록을 돌려준다.

use super::jsonc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// 엔트리를 따로 적지 않았을 때 찾아보는 파일 (순서대로 첫 번째).
const ENTRY_CANDIDATES: [&str; 5] = ["index.tsx", "index.jsx", "index.ts", "index.js", "index.html"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Module,
    Iframe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Powershell,
    Pwsh,
    Cmd,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parse {
    Text,
    Json,
    Lines,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Run {
    Line(String),
    Argv(Vec<String>),
}

/// 위젯이 선언한 명령. 실행 규칙은 `runner.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub run: Run,
    pub shell: Shell,
    /// 초. 0 = 시작할 때 한 번 + run_now 만.
    pub interval: u64,
    /// 초 (1~60)
    pub timeout: u64,
    pub parse: Parse,
    pub pause_when_hidden: bool,
}

/// 프론트로 나가는 정규화된 manifest.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub default_size: Size,
    pub min_size: Size,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub subscribe: Vec<String>,
    pub has_command: bool,
    #[serde(skip)]
    pub command: Option<CommandSpec>,
}

pub struct Parsed {
    pub manifest: Manifest,
    pub kind: Kind,
    /// 폴더 기준 상대 경로 (슬래시)
    pub entry: String,
    pub warnings: Vec<String>,
}

/// 폴더명 = 위젯 id. `^[a-z0-9][a-z0-9_-]{0,47}$`
pub fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    if b.is_empty() || b.len() > 48 {
        return false;
    }
    let ok = |c: u8, first: bool| c.is_ascii_lowercase() || c.is_ascii_digit() || (!first && (c == b'_' || c == b'-'));
    b.iter().enumerate().all(|(i, &c)| ok(c, i == 0))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawRun {
    Line(String),
    Argv(Vec<String>),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCommand {
    run: Option<RawRun>,
    shell: Option<String>,
    interval: Option<u64>,
    timeout: Option<u64>,
    parse: Option<String>,
    pause_when_hidden: Option<bool>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Deserialize)]
struct RawSize {
    w: u32,
    h: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawManifest {
    title: Option<String>,
    icon: Option<String>,
    description: Option<String>,
    entry: Option<String>,
    default_size: Option<RawSize>,
    min_size: Option<RawSize>,
    settings_schema: Option<Value>,
    provider: Option<String>,
    command: Option<RawCommand>,
    subscribe: Option<Vec<String>>,
    singleton: Option<Value>,
    #[serde(rename = "$schema")]
    _schema: Option<String>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

fn sorted_keys(m: &HashMap<String, Value>) -> Vec<&str> {
    let mut k: Vec<&str> = m.keys().map(String::as_str).collect();
    k.sort_unstable();
    k
}

/// 엔트리 확장자 → 종류. 모르는 확장자는 None.
pub fn kind_of(entry: &str) -> Option<Kind> {
    let ext = entry.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => Some(Kind::Iframe),
        "js" | "jsx" | "ts" | "tsx" | "mjs" => Some(Kind::Module),
        _ => None,
    }
}

/// 엔트리 경로가 폴더 밖으로 못 나가는 상대 경로인지.
fn safe_rel(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains(':')
        && !p.split('/').any(|s| s == ".." || s.is_empty())
}

fn command_from(raw: RawCommand, warnings: &mut Vec<String>) -> Result<CommandSpec, String> {
    for k in sorted_keys(&raw.extra) {
        warnings.push(format!("command: 알 수 없는 필드 `{k}`"));
    }
    let run = match raw.run {
        Some(RawRun::Line(s)) if !s.trim().is_empty() => Run::Line(s),
        Some(RawRun::Argv(v)) if !v.is_empty() && v.iter().all(|s| !s.is_empty()) => Run::Argv(v),
        _ => return Err("command.run 은 비어 있지 않은 문자열 또는 문자열 배열이어야 합니다".into()),
    };
    let shell = match raw.shell.as_deref().unwrap_or("powershell") {
        "powershell" => Shell::Powershell,
        "pwsh" => Shell::Pwsh,
        "cmd" => Shell::Cmd,
        "none" => Shell::None,
        other => return Err(format!("command.shell 은 powershell|pwsh|cmd|none 중 하나여야 합니다 (`{other}`)")),
    };
    if shell == Shell::None && !matches!(run, Run::Argv(_)) {
        return Err("command.shell 이 none 이면 command.run 은 배열(argv)이어야 합니다".into());
    }
    let parse = match raw.parse.as_deref().unwrap_or("text") {
        "text" => Parse::Text,
        "json" => Parse::Json,
        "lines" => Parse::Lines,
        other => return Err(format!("command.parse 는 text|json|lines 중 하나여야 합니다 (`{other}`)")),
    };
    let interval = match raw.interval {
        None => 60,
        Some(0) => 0,
        Some(n) => n.max(1),
    };
    let mut timeout = raw.timeout.unwrap_or(10).max(1);
    if timeout > 60 {
        warnings.push(format!("command.timeout {timeout}초는 최대 60초로 줄입니다"));
        timeout = 60;
    }
    Ok(CommandSpec { run, shell, interval, timeout, parse, pause_when_hidden: raw.pause_when_hidden.unwrap_or(true) })
}

/// `widget.json` 본문을 해석한다. `has_file` 은 폴더 안의 상대 경로가 파일로 있는지 묻는다.
pub fn parse(folder: &str, text: &str, has_file: &dyn Fn(&str) -> bool) -> Result<Parsed, String> {
    // 메모장·PowerShell 5 가 붙이는 BOM 은 JSON 이 아니다
    let stripped = jsonc::strip(text.strip_prefix('\u{feff}').unwrap_or(text));
    let raw: RawManifest = serde_json::from_str(&stripped)
        .map_err(|e| format!("widget.json {}:{} {}", e.line(), e.column(), e))?;
    let mut warnings = Vec::new();

    for k in sorted_keys(&raw.extra) {
        warnings.push(format!("알 수 없는 필드 `{k}`"));
    }
    if raw.singleton.is_some() {
        warnings.push("`singleton` 은 사용자 위젯에서 지원하지 않아 무시합니다".into());
    }

    let entry = match raw.entry {
        Some(e) => {
            let e = e.replace('\\', "/");
            if !safe_rel(&e) {
                return Err(format!("entry 경로가 올바르지 않습니다: {e}"));
            }
            if !has_file(&e) {
                return Err(format!("entry 파일이 없습니다: {e}"));
            }
            e
        }
        None => ENTRY_CANDIDATES
            .iter()
            .find(|c| has_file(c))
            .map(|c| c.to_string())
            .ok_or_else(|| "엔트리 파일이 없습니다 (index.tsx / index.jsx / index.ts / index.js / index.html)".to_string())?,
    };
    let kind = kind_of(&entry).ok_or_else(|| format!("지원하지 않는 entry 확장자입니다: {entry}"))?;

    let default_size = raw.default_size.map_or(Size { w: 240, h: 140 }, |s| Size { w: s.w.max(1), h: s.h.max(1) });
    let mut min_size = raw.min_size.map_or(Size { w: 120, h: 80 }, |s| Size { w: s.w.max(1), h: s.h.max(1) });
    if min_size.w > default_size.w || min_size.h > default_size.h {
        warnings.push("minSize 가 defaultSize 보다 커서 defaultSize 로 줄입니다".into());
        min_size = Size { w: min_size.w.min(default_size.w), h: min_size.h.min(default_size.h) };
    }

    let command = raw.command.map(|c| command_from(c, &mut warnings)).transpose()?;

    let manifest = Manifest {
        title: raw.title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| folder.to_string()),
        icon: raw.icon,
        description: raw.description,
        default_size,
        min_size,
        settings_schema: raw.settings_schema,
        provider: raw.provider,
        subscribe: raw.subscribe.unwrap_or_default(),
        has_command: command.is_some(),
        command,
    };
    Ok(Parsed { manifest, kind, entry, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |p| list.contains(&p)
    }

    #[test]
    fn id_rule() {
        for ok in ["clock", "a", "0x", "my_widget-2", &"a".repeat(48)] {
            assert!(valid_id(ok), "{ok}");
        }
        for bad in ["", "_x", "-x", "Clock", "a b", "a.b", "한글", &"a".repeat(49)] {
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn entry_inference_order_and_kind() {
        let p = parse("w", "{}", &files(&["index.jsx", "index.js", "index.html"])).unwrap();
        assert_eq!(p.entry, "index.jsx");
        assert_eq!(p.kind, Kind::Module);
        let p = parse("w", "{}", &files(&["index.html"])).unwrap();
        assert_eq!(p.kind, Kind::Iframe);
        assert!(parse("w", "{}", &files(&[])).is_err());
    }

    #[test]
    fn explicit_entry_checks() {
        let f = files(&["main.mjs", "page.htm", "x.txt"]);
        assert_eq!(parse("w", r#"{"entry":"main.mjs"}"#, &f).unwrap().kind, Kind::Module);
        assert_eq!(parse("w", r#"{"entry":"page.htm"}"#, &f).unwrap().kind, Kind::Iframe);
        assert!(parse("w", r#"{"entry":"x.txt"}"#, &f).is_err());
        assert!(parse("w", r#"{"entry":"../x.js"}"#, &f).is_err());
        assert!(parse("w", r#"{"entry":"nope.js"}"#, &f).is_err());
    }

    #[test]
    fn defaults_and_clamp() {
        let f = files(&["index.js"]);
        let p = parse("w", "{}", &f).unwrap();
        assert_eq!(p.manifest.title, "w");
        assert_eq!(p.manifest.default_size, Size { w: 240, h: 140 });
        assert_eq!(p.manifest.min_size, Size { w: 120, h: 80 });
        assert!(p.warnings.is_empty());

        let p = parse("w", r#"{"defaultSize":{"w":100,"h":200},"minSize":{"w":150,"h":50}}"#, &f).unwrap();
        assert_eq!(p.manifest.min_size, Size { w: 100, h: 50 });
        assert_eq!(p.warnings.len(), 1);
    }

    #[test]
    fn command_defaults_and_clamps() {
        let f = files(&["index.js"]);
        let p = parse("w", r#"{"command":{"run":"echo hi"}}"#, &f).unwrap();
        let c = p.manifest.command.unwrap();
        assert_eq!(c.shell, Shell::Powershell);
        assert_eq!((c.interval, c.timeout, c.parse, c.pause_when_hidden), (60, 10, Parse::Text, true));
        assert!(p.manifest.has_command);

        let p = parse("w", r#"{"command":{"run":"x","interval":0,"timeout":999,"parse":"json"}}"#, &f).unwrap();
        let c = p.manifest.command.unwrap();
        assert_eq!((c.interval, c.timeout, c.parse), (0, 60, Parse::Json));
        assert_eq!(p.warnings.len(), 1);

        let c = parse("w", r#"{"command":{"run":"x","timeout":0}}"#, &f).unwrap().manifest.command.unwrap();
        assert_eq!(c.timeout, 1);
        assert!(parse("w", r#"{"command":{"run":""}}"#, &f).is_err());
        assert!(parse("w", r#"{"command":{"run":"x","parse":"xml"}}"#, &f).is_err());
    }

    #[test]
    fn shell_none_requires_argv() {
        let f = files(&["index.js"]);
        assert!(parse("w", r#"{"command":{"run":"echo","shell":"none"}}"#, &f).is_err());
        let c = parse("w", r#"{"command":{"run":["git","status"],"shell":"none"}}"#, &f).unwrap().manifest.command.unwrap();
        assert_eq!(c.run, Run::Argv(vec!["git".into(), "status".into()]));
    }

    #[test]
    fn unknown_fields_and_singleton_warn() {
        let f = files(&["index.js"]);
        let p = parse("w", r#"{"$schema":"../widget.schema.json","foo":1,"singleton":true,"command":{"run":"x","bar":2}}"#, &f).unwrap();
        assert_eq!(p.warnings.len(), 3, "{:?}", p.warnings);
    }

    #[test]
    fn json_error_has_line_col_and_jsonc_allowed() {
        let f = files(&["index.js"]);
        let ok = parse("w", "{\n // c\n \"title\": \"T\",\n}", &f).unwrap();
        assert_eq!(ok.manifest.title, "T");
        let err = parse("w", "{\n  \"title\": oops\n}", &f).err().unwrap();
        assert!(err.contains("2:"), "{err}");
    }

    #[test]
    fn bom_from_notepad_is_ignored() {
        let f = files(&["index.js"]);
        assert_eq!(parse("w", "\u{feff}{ \"title\": \"T\" }", &f).unwrap().manifest.title, "T");
    }
}
