//! Claude(claude.ai) OAuth 로그인 — Claude Code 와 같은 PKCE 흐름, 토큰은 deskboard 전용 파일에 저장.
//!
//! 흐름: 인가 URL 을 브라우저로 열기 → 승인 페이지가 보여주는 `code#state` 를 위젯에 붙여넣기 → 토큰 교환.
//! Claude Code 의 `~/.claude/.credentials.json` 은 읽지도 쓰지도 않는다.
//! (비공식: Claude Code 의 공개 OAuth 클라이언트 id 를 사용한다.)

use crate::providers::oauth;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
const REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";
const SCOPES: &str = "org:create_api_key user:profile user:inference";
pub const TOKEN_URLS: [&str; 2] = [
    "https://platform.claude.com/v1/oauth/token",
    "https://console.anthropic.com/v1/oauth/token",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: String,
    /// unix ms
    pub expires_at: u64,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn token_path(app_data: &Path) -> PathBuf {
    app_data.join("claude.json")
}

pub fn load(path: &Path) -> Option<Token> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save(path: &Path, t: &Token) -> Result<(), String> {
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(t).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn clear(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// 진행 중인 로그인 (verifier, state).
#[derive(Default)]
pub struct Pending(pub Mutex<Option<(String, String)>>);

pub fn authorize_url(state: &str, challenge: &str) -> String {
    let mut u = url::Url::parse(AUTHORIZE_URL).unwrap();
    u.query_pairs_mut()
        .append_pair("code", "true")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", SCOPES)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);
    u.to_string()
}

/// 새 로그인을 시작해 인가 URL 을 돌려준다.
pub fn start(pending: &Pending) -> String {
    let (verifier, challenge) = oauth::pkce_pair();
    let state = oauth::random_state();
    let url = authorize_url(&state, &challenge);
    if let Ok(mut p) = pending.0.lock() {
        *p = Some((verifier, state));
    }
    url
}

/// 승인 페이지가 보여주는 값: `code#state` 또는 `code` 만. 공백/따옴표 제거.
pub fn parse_pasted(input: &str) -> (String, Option<String>) {
    let s = input.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    match s.split_once('#') {
        Some((c, st)) => (c.trim().to_string(), Some(st.trim().to_string()).filter(|x| !x.is_empty())),
        None => (s.to_string(), None),
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

async fn post_token(http: &reqwest::Client, payload: &Value, fallback_refresh: Option<&str>) -> Result<Token, String> {
    let mut last_err = String::from("토큰 요청 실패");
    for url in TOKEN_URLS {
        let res = http.post(url).json(payload).send().await.map_err(|e| format!("네트워크 오류: {e}"))?;
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        if status.is_success() {
            let t: TokenResponse = serde_json::from_str(&text).map_err(|_| "토큰 응답 형식 오류".to_string())?;
            let refresh = t.refresh_token.or_else(|| fallback_refresh.map(str::to_string)).ok_or("refresh_token 없음")?;
            return Ok(Token {
                access_token: t.access_token,
                refresh_token: refresh,
                expires_at: now_ms() + t.expires_in.unwrap_or(3600) * 1000,
            });
        }
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let detail = v.get("error_description").or_else(|| v.get("error")).and_then(Value::as_str).unwrap_or("").to_string();
        last_err = format!("토큰 요청 실패 ({status}{})", if detail.is_empty() { String::new() } else { format!(": {detail}") });
        log::warn!("claude oauth via {url}: {last_err}");
        if detail.contains("invalid_grant") || status.as_u16() == 400 {
            break;
        }
    }
    Err(last_err)
}

/// 붙여넣은 코드로 토큰을 교환하고 저장한다.
pub async fn finish(http: &reqwest::Client, pending: &Pending, path: &Path, pasted: &str) -> Result<Token, String> {
    let (verifier, state) = pending.0.lock().ok().and_then(|p| p.clone()).ok_or("먼저 [브라우저로 로그인] 을 누르세요")?;
    let (code, pasted_state) = parse_pasted(pasted);
    if code.is_empty() {
        return Err("코드가 비어 있습니다".into());
    }
    if let Some(ps) = &pasted_state {
        if ps != &state {
            return Err("state 값이 일치하지 않습니다. 로그인을 다시 시작하세요".into());
        }
    }
    let payload = serde_json::json!({
        "grant_type": "authorization_code",
        "code": code,
        "state": state,
        "client_id": CLIENT_ID,
        "redirect_uri": REDIRECT_URI,
        "code_verifier": verifier,
    });
    let token = post_token(http, &payload, None).await?;
    save(path, &token)?;
    if let Ok(mut p) = pending.0.lock() {
        *p = None;
    }
    Ok(token)
}

pub async fn refresh(http: &reqwest::Client, path: &Path, token: &Token) -> Result<Token, String> {
    let payload = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": token.refresh_token,
        "client_id": CLIENT_ID,
    });
    let t = post_token(http, &payload, Some(&token.refresh_token)).await?;
    save(path, &t)?;
    log::info!("claude oauth token refreshed");
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pasted_code_variants() {
        assert_eq!(parse_pasted("abc#xyz"), ("abc".into(), Some("xyz".into())));
        assert_eq!(parse_pasted("  \"abc\" "), ("abc".into(), None));
        assert_eq!(parse_pasted("abc#"), ("abc".into(), None));
    }

    #[test]
    fn authorize_url_has_pkce() {
        let u = authorize_url("st", "ch");
        assert!(u.starts_with("https://claude.ai/oauth/authorize?code=true&client_id="));
        assert!(u.contains("code_challenge=ch") && u.contains("state=st") && u.contains("code_challenge_method=S256"));
    }

    #[test]
    fn token_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("deskboard-claude-{}", std::process::id()));
        let p = token_path(&dir);
        let t = Token { access_token: "a".into(), refresh_token: "r".into(), expires_at: 123 };
        save(&p, &t).unwrap();
        assert_eq!(load(&p), Some(t));
        clear(&p);
        assert_eq!(load(&p), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn finish_rejects_state_mismatch() {
        let pending = Pending::default();
        let _ = start(&pending);
        let http = reqwest::Client::new();
        let err = finish(&http, &pending, Path::new("x"), "code#wrong").await.unwrap_err();
        assert!(err.contains("state"));
    }
}
