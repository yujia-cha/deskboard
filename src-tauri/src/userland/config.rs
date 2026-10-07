//! 데이터 폴더의 사용자 설정 파일 — `config.jsonc` · `strings.jsonc` · `user.css`.
//!
//! 없으면 기본값(None), 깨져 있으면 무시하고(None) 오류를 모아 프론트에 알린다. **패닉하지 않는다.**
//! 실제 파일은 앱이 만들거나 고치지 않는다 (`seed.rs` 는 `*.sample.*` 만 쓴다).

use super::jsonc;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

pub const CONFIG_FILE: &str = "config.jsonc";
pub const STRINGS_FILE: &str = "strings.jsonc";
pub const CSS_FILE: &str = "user.css";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConfigError {
    pub file: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSnapshot {
    pub config: Option<Value>,
    pub strings: Option<Value>,
    pub css: Option<String>,
    pub errors: Vec<ConfigError>,
    pub dir: String,
}

/// 현재 설정. 폴링 루프와 트레이가 읽는다.
static CURRENT: OnceLock<RwLock<ConfigSnapshot>> = OnceLock::new();

fn cell() -> &'static RwLock<ConfigSnapshot> {
    CURRENT.get_or_init(|| RwLock::new(ConfigSnapshot::default()))
}

pub fn snapshot() -> ConfigSnapshot {
    cell().read().map(|g| g.clone()).unwrap_or_default()
}

pub fn store(s: ConfigSnapshot) {
    if let Ok(mut g) = cell().write() {
        *g = s;
    }
}

/// 파일 하나를 읽는다. 없으면 `Ok(None)`. 메모장·PowerShell 5 가 붙이는 BOM 은 뗀다
/// (CSS 맨 앞에 남으면 첫 규칙이 깨진다).
fn read_text(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(t))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("읽지 못했습니다: {e}")),
    }
}

/// JSONC 파일 → 객체. 빈 파일은 없는 것으로 본다.
fn read_jsonc(dir: &Path, name: &str, errors: &mut Vec<ConfigError>) -> Option<Value> {
    let mut fail = |message: String| errors.push(ConfigError { file: name.to_string(), message });
    let text = match read_text(&dir.join(name)) {
        Ok(Some(t)) => t,
        Ok(None) => return None,
        Err(m) => {
            fail(m);
            return None;
        }
    };
    let stripped = jsonc::strip(&text);
    if stripped.trim().is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(&stripped) {
        Ok(v) if v.is_object() => Some(v),
        Ok(_) => {
            fail("1:1 최상위 값은 객체({ })여야 합니다".into());
            None
        }
        Err(e) => {
            fail(format!("{}:{} {}", e.line(), e.column(), e));
            None
        }
    }
}

pub fn load(dir: &Path) -> ConfigSnapshot {
    let mut errors = Vec::new();
    let config = read_jsonc(dir, CONFIG_FILE, &mut errors);
    let strings = read_jsonc(dir, STRINGS_FILE, &mut errors);
    let css = match read_text(&dir.join(CSS_FILE)) {
        Ok(t) => t,
        Err(message) => {
            errors.push(ConfigError { file: CSS_FILE.into(), message });
            None
        }
    };
    ConfigSnapshot { config, strings, css, errors, dir: dir.to_string_lossy().to_string() }
}

/// `config.intervals.<key>`(초)에서 주기를 읽는다. 숫자가 아니거나 0 이하면 기본값, `min` 보다 작으면 `min`.
pub fn interval_from(config: Option<&Value>, key: &str, default: Duration, min: Duration) -> Duration {
    let secs = config
        .and_then(|c| c.get("intervals"))
        .and_then(|i| i.get(key))
        .and_then(Value::as_f64)
        .filter(|s| s.is_finite() && *s > 0.0)
        // 너무 큰 값은 `from_secs_f64` 를 터뜨린다 (폴링 스레드가 죽는다) — 하루로 자른다.
        .map(|s| s.min(86_400.0));
    let d = secs.map_or(default, Duration::from_secs_f64);
    d.max(min)
}

/// 폴링 루프가 매 주기 부른다. 설정을 고치면 다음 주기부터 바로 반영된다.
pub fn interval(key: &str, default: Duration, min: Duration) -> Duration {
    let g = cell().read();
    interval_from(g.as_ref().ok().and_then(|g| g.config.as_ref()), key, default, min)
}

/// `strings.tray.<key>`. 문자열이 아니거나 비어 있으면 `default`.
pub fn tray_string_from(strings: Option<&Value>, key: &str, default: &str) -> String {
    strings
        .and_then(|s| s.get("tray"))
        .and_then(|t| t.get(key))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(default)
        .to_string()
}

pub fn tray_text(key: &str, default: &str) -> String {
    let g = cell().read();
    tray_string_from(g.as_ref().ok().and_then(|g| g.strings.as_ref()), key, default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn missing_files_give_defaults() {
        let t = TempDir::new().unwrap();
        let s = load(t.path());
        assert!(s.config.is_none() && s.strings.is_none() && s.css.is_none() && s.errors.is_empty());
    }

    #[test]
    fn reads_jsonc_and_css() {
        let t = TempDir::new().unwrap();
        std::fs::write(t.path().join(CONFIG_FILE), "{ // c\n \"intervals\": { \"sysmon\": 3, }, }").unwrap();
        std::fs::write(t.path().join(CSS_FILE), ":root{}").unwrap();
        let s = load(t.path());
        assert_eq!(s.config.unwrap()["intervals"]["sysmon"], 3);
        assert_eq!(s.css.as_deref(), Some(":root{}"));
        assert!(s.errors.is_empty());
    }

    #[test]
    fn broken_file_reports_line_and_keeps_others() {
        let t = TempDir::new().unwrap();
        std::fs::write(t.path().join(CONFIG_FILE), "{\n  \"a\": 1,\n  \"b\": nope\n}").unwrap();
        std::fs::write(t.path().join(STRINGS_FILE), r#"{"tray":{"quit":"Q"}}"#).unwrap();
        let s = load(t.path());
        assert!(s.config.is_none());
        assert!(s.strings.is_some());
        assert_eq!(s.errors.len(), 1);
        assert_eq!(s.errors[0].file, CONFIG_FILE);
        assert!(s.errors[0].message.starts_with("3:"), "{}", s.errors[0].message);
    }

    #[test]
    fn non_object_and_blank_files() {
        let t = TempDir::new().unwrap();
        std::fs::write(t.path().join(CONFIG_FILE), "[1]").unwrap();
        std::fs::write(t.path().join(STRINGS_FILE), "  // 비어 있음\n").unwrap();
        let s = load(t.path());
        assert!(s.config.is_none() && s.strings.is_none());
        assert_eq!(s.errors.len(), 1);
    }

    #[test]
    fn tray_string_fallback() {
        let v = json!({"tray": {"quit": "Bye", "show": "  ", "settings": 3}});
        assert_eq!(tray_string_from(Some(&v), "quit", "종료"), "Bye");
        assert_eq!(tray_string_from(Some(&v), "show", "표시"), "표시");
        assert_eq!(tray_string_from(Some(&v), "settings", "설정"), "설정");
        assert_eq!(tray_string_from(Some(&v), "tooltip", "deskboard"), "deskboard");
        assert_eq!(tray_string_from(None, "quit", "종료"), "종료");
    }

    #[test]
    fn interval_clamps_and_defaults() {
        let c = json!({"intervals": {"a": 10, "b": 0.2, "c": "x", "d": -5, "e": 0}});
        assert_eq!(interval_from(Some(&c), "a", secs(2), secs(1)), secs(10));
        assert_eq!(interval_from(Some(&c), "b", secs(2), secs(1)), secs(1)); // min 으로 올림
        assert_eq!(interval_from(Some(&c), "c", secs(2), secs(1)), secs(2));
        assert_eq!(interval_from(Some(&c), "d", secs(2), secs(1)), secs(2));
        assert_eq!(interval_from(Some(&c), "e", secs(2), secs(1)), secs(2));
        assert_eq!(interval_from(Some(&c), "none", secs(2), secs(1)), secs(2));
        assert_eq!(interval_from(None, "a", secs(2), secs(1)), secs(2));
        // 기본값이 min 보다 작아도 min 이 이긴다
        assert_eq!(interval_from(None, "a", secs(1), secs(5)), secs(5));
        // Duration 을 넘치는 값도 터지지 않는다
        let huge = json!({ "intervals": { "a": 1e30 } });
        assert_eq!(interval_from(Some(&huge), "a", secs(2), secs(1)), secs(86_400));
    }
}
