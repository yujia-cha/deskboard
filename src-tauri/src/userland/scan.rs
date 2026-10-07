//! 위젯 루트(내장·사용자) 스캔과 번들 만들기.
//!
//! 폴더명 = 위젯 id. 같은 id 가 두 루트에 있으면 사용자 루트가 이긴다.
//! `_`/`.` 로 시작하는 폴더는 조용히 무시한다 (비활성화 규칙). 오류가 난 폴더도 **목록에서 빼지 않고**
//! `error` 를 달아 남긴다 — 사용자가 왜 안 뜨는지 볼 수 있어야 한다.

use super::manifest::{self, CommandSpec, Kind, Manifest};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

const MAX_DEPTH: usize = 4;
const MAX_FILE: u64 = 512 * 1024;
const MAX_TOTAL: u64 = 4 * 1024 * 1024;
const BUNDLE_EXTS: [&str; 7] = ["js", "jsx", "ts", "tsx", "mjs", "json", "css"];

#[derive(Debug, Clone)]
pub struct Roots {
    pub builtin: PathBuf,
    pub user: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Root {
    Builtin,
    User,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetInfo {
    pub id: String,
    pub root: Root,
    pub kind: Option<Kind>,
    pub entry: Option<String>,
    pub manifest: Option<Manifest>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
    pub version: String,
    pub overrides_builtin: bool,
    pub dir: String,
}

/// 백엔드 내부용: 프론트로 나가는 `WidgetInfo` + 실행할 명령.
#[derive(Debug, Clone)]
pub struct Scanned {
    pub info: WidgetInfo,
    pub command: Option<CommandSpec>,
}

#[derive(Debug, Serialize)]
pub struct Bundle {
    pub entry: String,
    pub files: HashMap<String, String>,
}

fn ignored_name(name: &str) -> bool {
    name.starts_with('_') || name.starts_with('.')
}

fn is_test_file(name: &str) -> bool {
    name.contains(".test.")
}

/// 루트 바로 아래의 폴더들. 루트가 없으면 빈 목록.
fn list_dirs(root: &Path) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(root) else {
        log::warn!("위젯 폴더를 읽지 못했습니다: {}", root.display());
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .filter(|(n, _)| !ignored_name(n))
        .collect()
}

/// 번들·버전 계산이 보는 파일들 (폴더 기준 상대 경로 슬래시, 절대 경로). 정렬돼 있다.
fn walk_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = WalkDir::new(dir)
        .max_depth(MAX_DEPTH)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            !(ignored_name(&name) || (e.file_type().is_dir() && name == "node_modules"))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| !is_test_file(&e.file_name().to_string_lossy()))
        .filter_map(|e| {
            let rel = e.path().strip_prefix(dir).ok()?;
            let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            Some((rel, e.into_path()))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// (상대 경로, mtime, 길이) 의 해시. 파일이 바뀌면 달라진다.
fn version_of(dir: &Path) -> String {
    let mut h = Sha256::new();
    for (rel, path) in walk_files(dir) {
        let (mtime, len) = std::fs::metadata(&path)
            .map(|m| {
                let t = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
                (t, m.len())
            })
            .unwrap_or((0, 0));
        h.update(rel.as_bytes());
        h.update([0]);
        h.update(mtime.to_le_bytes());
        h.update(len.to_le_bytes());
    }
    let d = h.finalize();
    d.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn scan_one(id: &str, dir: &Path, root: Root, overrides_builtin: bool) -> Scanned {
    let mut info = WidgetInfo {
        id: id.to_string(),
        root,
        kind: None,
        entry: None,
        manifest: None,
        error: None,
        warnings: Vec::new(),
        version: version_of(dir),
        overrides_builtin,
        dir: dir.to_string_lossy().to_string(),
    };
    let mut command = None;
    if !manifest::valid_id(id) {
        info.error = Some("폴더 이름은 소문자·숫자·`_`·`-` 만, 영문/숫자로 시작해 48자 이하여야 합니다".into());
        return Scanned { info, command };
    }
    let text = match std::fs::read_to_string(dir.join("widget.json")) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            info.error = Some("widget.json 이 없습니다".into());
            return Scanned { info, command };
        }
        Err(e) => {
            info.error = Some(format!("widget.json 을 읽지 못했습니다: {e}"));
            return Scanned { info, command };
        }
    };
    match manifest::parse(id, &text, &|rel| dir.join(rel).is_file()) {
        Ok(p) => {
            info.kind = Some(p.kind);
            info.entry = Some(p.entry);
            info.warnings = p.warnings;
            command = p.manifest.command.clone();
            info.manifest = Some(p.manifest);
        }
        Err(e) => info.error = Some(e),
    }
    for w in &info.warnings {
        log::warn!("위젯 `{id}`: {w}");
    }
    Scanned { info, command }
}

/// 두 루트를 합쳐 id 순으로 돌려준다.
pub fn scan(roots: &Roots) -> Vec<Scanned> {
    let mut merged: BTreeMap<String, (Root, PathBuf, bool)> = BTreeMap::new();
    for (name, dir) in list_dirs(&roots.builtin) {
        merged.insert(name, (Root::Builtin, dir, false));
    }
    for (name, dir) in list_dirs(&roots.user) {
        let over = merged.get(&name).is_some_and(|(r, _, _)| *r == Root::Builtin);
        merged.insert(name, (Root::User, dir, over));
    }
    merged.into_iter().map(|(id, (root, dir, over))| scan_one(&id, &dir, root, over)).collect()
}

/// 모듈 위젯의 소스 번들. 오류는 사용자에게 보일 메시지로 돌려준다.
pub fn bundle(dir: &Path, entry: &str) -> Result<Bundle, String> {
    let mut files = HashMap::new();
    let mut total: u64 = 0;
    for (rel, path) in walk_files(dir) {
        let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !BUNDLE_EXTS.contains(&ext.as_str()) {
            continue;
        }
        let len = std::fs::metadata(&path).map_err(|e| format!("{rel}: {e}"))?.len();
        if len > MAX_FILE {
            return Err(format!("{rel} 이(가) 너무 큽니다 ({}KB, 파일당 최대 512KB)", len / 1024));
        }
        total += len;
        if total > MAX_TOTAL {
            return Err("위젯 파일 합계가 4MB 를 넘습니다".into());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{rel} 을(를) 읽지 못했습니다: {e}"))?;
        // BOM 이 남으면 JSON.parse 가 실패하고 CSS 첫 규칙이 깨진다
        files.insert(rel, text.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(text));
    }
    if !files.contains_key(entry) {
        return Err(format!("엔트리 파일을 찾지 못했습니다: {entry}"));
    }
    Ok(Bundle { entry: entry.to_string(), files })
}

/// 폴더를 통째로 복사한다 (`*.test.*` 제외). 내장 위젯 "복사해서 고치기"에 쓴다.
pub fn copy_dir(src: &Path, dest: &Path) -> Result<(), String> {
    for e in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let rel = e.path().strip_prefix(src).map_err(|e| e.to_string())?;
        let target = dest.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else if e.file_type().is_file() && !is_test_file(&e.file_name().to_string_lossy()) {
            std::fs::copy(e.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn w(root: &Path, id: &str, manifest: Option<&str>, entry: Option<&str>) {
        let d = root.join(id);
        std::fs::create_dir_all(&d).unwrap();
        if let Some(m) = manifest {
            std::fs::write(d.join("widget.json"), m).unwrap();
        }
        if let Some(e) = entry {
            std::fs::write(d.join(e), "export default () => null").unwrap();
        }
    }

    fn setup() -> (TempDir, Roots) {
        let t = TempDir::new().unwrap();
        let roots = Roots { builtin: t.path().join("builtin"), user: t.path().join("user") };
        std::fs::create_dir_all(&roots.builtin).unwrap();
        std::fs::create_dir_all(&roots.user).unwrap();
        (t, roots)
    }

    #[test]
    fn merges_roots_and_user_overrides() {
        let (_t, r) = setup();
        w(&r.builtin, "clock", Some(r#"{"title":"B"}"#), Some("index.tsx"));
        w(&r.builtin, "cpu", Some("{}"), Some("index.tsx"));
        w(&r.user, "clock", Some(r#"{"title":"U"}"#), Some("index.jsx"));
        w(&r.user, "mine", Some("{}"), Some("index.html"));
        let list = scan(&r);
        let ids: Vec<_> = list.iter().map(|s| s.info.id.as_str()).collect();
        assert_eq!(ids, ["clock", "cpu", "mine"]);
        let clock = &list[0].info;
        assert_eq!(clock.root, Root::User);
        assert!(clock.overrides_builtin);
        assert_eq!(clock.manifest.as_ref().unwrap().title, "U");
        assert_eq!(list[1].info.root, Root::Builtin);
        assert!(!list[1].info.overrides_builtin);
        assert_eq!(list[2].info.kind, Some(Kind::Iframe));
        assert!(!list[2].info.overrides_builtin);
    }

    #[test]
    fn broken_folders_stay_as_error_entries() {
        let (_t, r) = setup();
        w(&r.user, "nomanifest", None, Some("index.js"));
        w(&r.user, "badjson", Some("{ oops"), Some("index.js"));
        w(&r.user, "noentry", Some("{}"), None);
        w(&r.user, "BadName", Some("{}"), Some("index.js"));
        let list = scan(&r);
        assert_eq!(list.len(), 4);
        assert!(list.iter().all(|s| s.info.error.is_some() && s.info.manifest.is_none()));
        let bad = list.iter().find(|s| s.info.id == "badjson").unwrap();
        assert!(bad.info.error.as_ref().unwrap().contains("1:"));
    }

    #[test]
    fn underscore_and_dot_ignored_silently() {
        let (_t, r) = setup();
        w(&r.user, "_off", Some("{}"), Some("index.js"));
        w(&r.user, ".git", Some("{}"), Some("index.js"));
        w(&r.builtin, "_hidden", Some("{}"), Some("index.js"));
        std::fs::write(r.user.join("README.md"), "x").unwrap();
        assert!(scan(&r).is_empty());
    }

    #[test]
    fn missing_roots_are_empty() {
        let roots = Roots { builtin: PathBuf::from("Z:/nope-a"), user: PathBuf::from("Z:/nope-b") };
        assert!(scan(&roots).is_empty());
    }

    #[test]
    fn version_changes_with_content() {
        let (_t, r) = setup();
        w(&r.user, "a", Some("{}"), Some("index.js"));
        let v1 = scan(&r)[0].info.version.clone();
        std::fs::write(r.user.join("a").join("index.js"), "export default () => 1; // longer").unwrap();
        let v2 = scan(&r)[0].info.version.clone();
        assert_ne!(v1, v2);
    }

    #[test]
    fn bundle_filters_and_limits() {
        let (_t, r) = setup();
        w(&r.user, "a", Some("{}"), Some("index.jsx"));
        let d = r.user.join("a");
        std::fs::write(d.join("util.ts"), "1").unwrap();
        std::fs::write(d.join("style.css"), "1").unwrap();
        std::fs::write(d.join("x.test.ts"), "1").unwrap();
        std::fs::write(d.join("pic.png"), "1").unwrap();
        std::fs::create_dir_all(d.join("node_modules")).unwrap();
        std::fs::write(d.join("node_modules").join("m.js"), "1").unwrap();
        std::fs::create_dir_all(d.join("_old")).unwrap();
        std::fs::write(d.join("_old").join("o.js"), "1").unwrap();
        std::fs::create_dir_all(d.join("lib")).unwrap();
        std::fs::write(d.join("lib").join("h.js"), "1").unwrap();
        let b = bundle(&d, "index.jsx").unwrap();
        let mut keys: Vec<_> = b.files.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["index.jsx", "lib/h.js", "style.css", "util.ts", "widget.json"]);

        std::fs::write(d.join("big.js"), vec![b'a'; 513 * 1024]).unwrap();
        assert!(bundle(&d, "index.jsx").unwrap_err().contains("big.js"));
    }

    #[test]
    fn copy_skips_tests() {
        let (t, r) = setup();
        w(&r.builtin, "a", Some("{}"), Some("index.js"));
        std::fs::write(r.builtin.join("a").join("a.test.ts"), "1").unwrap();
        std::fs::create_dir_all(r.builtin.join("a").join("sub")).unwrap();
        std::fs::write(r.builtin.join("a").join("sub").join("s.js"), "1").unwrap();
        let dest = t.path().join("out");
        copy_dir(&r.builtin.join("a"), &dest).unwrap();
        assert!(dest.join("index.js").is_file() && dest.join("sub").join("s.js").is_file());
        assert!(!dest.join("a.test.ts").exists());
    }
}
