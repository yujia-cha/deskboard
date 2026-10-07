//! 커스텀 URI 스킴 `dbw` — 위젯 폴더의 파일을 웹뷰(특히 sandbox iframe)에 서빙한다.
//!
//! 경로는 `/<id>/<상대 경로>`. WebView2 에서는 `http://dbw.localhost/<id>/<경로>` 로 보인다.
//! 요청 경로는 신뢰하지 않는다 — `resolve` 가 위젯 폴더 밖으로 나가는 모든 시도를 거른다.

use super::Userland;
use std::path::{Path, PathBuf};
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};
use percent_encoding::percent_decode_str;

/// `resolve` 의 거절 사유.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    BadRequest,
    Forbidden,
    NotFound,
}

impl Status {
    fn code(self) -> StatusCode {
        match self {
            Status::BadRequest => StatusCode::BAD_REQUEST,
            Status::Forbidden => StatusCode::FORBIDDEN,
            Status::NotFound => StatusCode::NOT_FOUND,
        }
    }
}

/// 예약된 경로 접두사 (`/_sdk/…`). 위젯 id 는 `_` 로 시작할 수 없어 충돌하지 않는다.
const SDK_PREFIX: &str = "_sdk";

/// 요청 경로를 `(위젯 id, 폴더 기준 상대 경로)` 로 쪼갠다. `..` · 절대 경로 · 역슬래시는 거절.
fn split(uri_path: &str) -> Result<(String, Vec<String>), Status> {
    let decoded = percent_decode_str(uri_path).decode_utf8().map_err(|_| Status::BadRequest)?;
    // 디코딩 **뒤에** 검사한다 — `%2e%2e`, `%5c` 로 우회하지 못하게.
    if decoded.contains('\\') || decoded.contains('\0') || decoded.contains(':') {
        return Err(Status::Forbidden);
    }
    let rest = decoded.strip_prefix('/').unwrap_or(&decoded);
    if rest.is_empty() {
        return Err(Status::NotFound);
    }
    let mut segs: Vec<&str> = rest.split('/').collect();
    if segs.contains(&"..") {
        return Err(Status::Forbidden);
    }
    // 빈 조각(`//`)이나 `.` 은 건너뛴다. 단, 맨 앞이 비면(`//x`) 절대 경로 시도로 본다.
    if segs.first().is_some_and(|s| s.is_empty()) {
        return Err(Status::Forbidden);
    }
    segs.retain(|s| !s.is_empty() && *s != ".");
    let mut it = segs.into_iter();
    let id = it.next().ok_or(Status::NotFound)?.to_string();
    let rel: Vec<String> = it.map(str::to_string).collect();
    if rel.is_empty() {
        return Err(Status::NotFound);
    }
    Ok((id, rel))
}

/// 순수 함수: 요청 경로 → 서빙할 실제 파일. `lookup` 은 위젯 id → 그 위젯의 (유효한) 폴더.
pub fn resolve(lookup: &dyn Fn(&str) -> Option<PathBuf>, uri_path: &str) -> Result<PathBuf, Status> {
    let (id, rel) = split(uri_path)?;
    if id == SDK_PREFIX {
        return Err(Status::NotFound);
    }
    let base = lookup(&id).ok_or(Status::NotFound)?;
    let base = base.canonicalize().map_err(|_| Status::NotFound)?;
    let mut full = base.clone();
    for s in &rel {
        full.push(s);
    }
    let canon = full.canonicalize().map_err(|_| Status::NotFound)?;
    // symlink/junction 으로 폴더 밖을 가리키면 여기서 걸린다.
    if !canon.starts_with(&base) {
        return Err(Status::Forbidden);
    }
    if !canon.is_file() {
        return Err(Status::NotFound);
    }
    Ok(canon)
}

pub fn mime_of(path: &Path) -> &'static str {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// HTML 위젯이 대시보드와 이야기하는 통로 (`window.deskboard`). 셸 쪽 짝은 `src/components/FrameWidget.tsx`.
const BRIDGE_JS: &str = include_str!("../../resources/userland/sdk/bridge.js");
const BRIDGE_TAG: &str = "<script src=\"/_sdk/bridge.js\"></script>";

/// `/_sdk/<파일>` — 호스트가 제공하는 자산.
fn sdk_asset(rel: &str) -> Option<(&'static str, Vec<u8>)> {
    match rel {
        "bridge.js" => Some(("text/javascript; charset=utf-8", BRIDGE_JS.as_bytes().to_vec())),
        _ => None,
    }
}

/// HTML 위젯에 브리지 스크립트를 넣는다 — 위젯 작성자가 직접 넣지 않아도 되게.
/// `<head>` 안 맨 앞(위젯 스크립트보다 먼저)에, head 가 없으면 문서 맨 앞에.
fn inject_bridge(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    if let Some(start) = lower.find("<head") {
        if let Some(end) = lower[start..].find('>') {
            let at = start + end + 1;
            return format!("{}{}{}", &html[..at], BRIDGE_TAG, &html[at..]);
        }
    }
    // <!doctype> 뒤에 두어야 표준 모드가 유지된다
    if lower.trim_start().starts_with("<!doctype") {
        if let Some(end) = lower.find('>') {
            return format!("{}{}{}", &html[..=end], BRIDGE_TAG, &html[end + 1..]);
        }
    }
    format!("{BRIDGE_TAG}{html}")
}

fn respond(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn error(status: Status) -> Response<Vec<u8>> {
    respond(status.code(), "text/plain; charset=utf-8", status.code().to_string().into_bytes())
}

/// `register_asynchronous_uri_scheme_protocol("dbw", …)` 에 꽂는 핸들러.
pub fn handle<R: Runtime>(ctx: UriSchemeContext<'_, R>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    let path = request.uri().path().to_string();
    // 파일 읽기는 웹뷰 스레드를 막지 않게 blocking 풀에서 한다.
    tauri::async_runtime::spawn_blocking(move || {
        let resp = serve(&app, &path);
        responder.respond(resp);
    });
}

fn serve<R: Runtime>(app: &tauri::AppHandle<R>, path: &str) -> Response<Vec<u8>> {
    if let Ok((id, rel)) = split(path) {
        if id == SDK_PREFIX {
            return match sdk_asset(&rel.join("/")) {
                Some((mime, body)) => respond(StatusCode::OK, mime, body),
                None => error(Status::NotFound),
            };
        }
    }
    let Some(ul) = app.try_state::<Userland>() else {
        return error(Status::NotFound);
    };
    let file = match resolve(&|id| ul.widget_dir(id), path) {
        Ok(f) => f,
        Err(s) => return error(s),
    };
    match std::fs::read(&file) {
        Ok(body) => {
            let mime = mime_of(&file);
            if mime.starts_with("text/html") {
                let html = String::from_utf8_lossy(&body);
                return respond(StatusCode::OK, mime, inject_bridge(&html).into_bytes());
            }
            respond(StatusCode::OK, mime, body)
        }
        Err(_) => error(Status::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, PathBuf) {
        let t = TempDir::new().unwrap();
        let w = t.path().join("w");
        std::fs::create_dir_all(w.join("sub")).unwrap();
        std::fs::write(w.join("index.html"), "<html></html>").unwrap();
        std::fs::write(w.join("sub").join("a.css"), "a{}").unwrap();
        std::fs::write(t.path().join("secret.txt"), "secret").unwrap();
        (t, w)
    }

    #[test]
    fn serves_files_inside_widget() {
        let (_t, w) = fixture();
        let lookup = |id: &str| (id == "w").then(|| w.clone());
        let p = resolve(&lookup, "/w/index.html").unwrap();
        assert!(p.ends_with("index.html"));
        assert!(resolve(&lookup, "/w/sub/a.css").unwrap().ends_with("a.css"));
        assert!(resolve(&lookup, "/w/sub/./a.css").is_ok());
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        let (_t, w) = fixture();
        let lookup = |id: &str| (id == "w").then(|| w.clone());
        for bad in [
            "/w/../secret.txt",
            "/w/sub/../../secret.txt",
            "/w/%2e%2e/secret.txt",
            "/w/%2E%2E%2fsecret.txt",
            "/w/..%5csecret.txt",
            "/w/sub\\a.css",
            "/w/C:/Windows/win.ini",
            "//w/index.html",
            "/w/%00",
        ] {
            assert_eq!(resolve(&lookup, bad), Err(Status::Forbidden), "{bad}");
        }
        assert_eq!(resolve(&lookup, "/w/%ff"), Err(Status::BadRequest));
    }

    #[test]
    fn not_found_cases() {
        let (_t, w) = fixture();
        let lookup = |id: &str| (id == "w").then(|| w.clone());
        assert_eq!(resolve(&lookup, "/w/missing.js"), Err(Status::NotFound));
        assert_eq!(resolve(&lookup, "/other/index.html"), Err(Status::NotFound));
        assert_eq!(resolve(&lookup, "/w"), Err(Status::NotFound));
        assert_eq!(resolve(&lookup, "/w/sub"), Err(Status::NotFound)); // 디렉터리
        assert_eq!(resolve(&lookup, "/"), Err(Status::NotFound));
        assert_eq!(resolve(&lookup, "/_sdk/bridge.js"), Err(Status::NotFound));
    }

    #[test]
    fn symlink_escape_is_forbidden() {
        let (t, w) = fixture();
        let link = w.join("link.txt");
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(t.path().join("secret.txt"), &link).is_ok();
        #[cfg(not(windows))]
        let made = std::os::unix::fs::symlink(t.path().join("secret.txt"), &link).is_ok();
        if !made {
            eprintln!("symlink 권한이 없어 건너뜀");
            return;
        }
        let lookup = |id: &str| (id == "w").then(|| w.clone());
        assert_eq!(resolve(&lookup, "/w/link.txt"), Err(Status::Forbidden));
    }

    #[test]
    fn mime_table() {
        for (f, m) in [
            ("a.html", "text/html"),
            ("a.css", "text/css"),
            ("a.js", "text/javascript"),
            ("a.mjs", "text/javascript"),
            ("a.json", "application/json"),
            ("a.PNG", "image/png"),
            ("a.jpeg", "image/jpeg"),
            ("a.svg", "image/svg+xml"),
            ("a.woff2", "font/woff2"),
            ("a.txt", "text/plain"),
            ("a.bin", "application/octet-stream"),
        ] {
            assert!(mime_of(Path::new(f)).starts_with(m), "{f}");
        }
    }

    #[test]
    fn bridge_goes_first_in_head() {
        let out = inject_bridge("<!DOCTYPE html><html><HEAD lang=\"ko\"><script src=\"app.js\"></script></HEAD></html>");
        let bridge = out.find(BRIDGE_TAG).unwrap();
        assert!(bridge < out.find("app.js").unwrap(), "위젯 스크립트보다 먼저 와야 한다");
        assert!(out.starts_with("<!DOCTYPE html><html><HEAD lang=\"ko\">"));
    }

    #[test]
    fn bridge_without_head_keeps_doctype_first() {
        assert!(inject_bridge("<!doctype html><body>x</body>").starts_with("<!doctype html><script"));
        assert!(inject_bridge("<div>x</div>").starts_with(BRIDGE_TAG));
    }

    #[test]
    fn sdk_serves_only_the_bridge() {
        assert!(sdk_asset("bridge.js").is_some());
        assert!(sdk_asset("../secret").is_none());
    }
}
