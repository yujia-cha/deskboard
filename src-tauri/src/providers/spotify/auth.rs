//! Spotify OAuth — Authorization Code + PKCE (Client Secret 불필요).
//! spotify-tui 의 `auth.py` 를 포팅했다: 루프백 서버로 리다이렉트를 받고, `state` 검증 후 토큰 교환.

use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SCOPES: &str = "user-read-playback-state user-modify-playback-state user-read-currently-playing playlist-read-private playlist-read-collaborative user-read-private";
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthFile {
    pub client_id: String,
    #[serde(default = "default_redirect")]
    pub redirect_uri: String,
    pub token: Option<Token>,
}
fn default_redirect() -> String {
    DEFAULT_REDIRECT_URI.into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: String,
    /// unix seconds
    pub expires_at: u64,
}

impl Token {
    pub fn expires_soon(&self) -> bool {
        now_secs() + 60 >= self.expires_at
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("{0}")]
    Invalid(String),
    #[error("로그인을 취소했습니다")]
    Cancelled,
    #[error("시간 안에 브라우저 승인이 오지 않았습니다")]
    Timeout,
    #[error("네트워크 오류: {0}")]
    Http(#[from] reqwest::Error),
    #[error("파일 오류: {0}")]
    Io(#[from] std::io::Error),
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn pkce_pair() -> (String, String) {
    let mut raw = [0u8; 64];
    rand::rng().fill_bytes(&mut raw);
    let verifier = b64url(&raw);
    let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

pub fn random_state() -> String {
    let mut raw = [0u8; 24];
    rand::rng().fill_bytes(&mut raw);
    b64url(&raw)
}

/// 리다이렉트 URI 에서 바인딩할 (host, port). 루프백만 허용.
pub fn parse_redirect(uri: &str) -> Result<(String, u16), AuthError> {
    let u = url::Url::parse(uri).map_err(|_| AuthError::Invalid("Redirect URI 형식이 잘못됐습니다".into()))?;
    let host = u.host_str().unwrap_or("");
    if u.scheme() != "http" || !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        return Err(AuthError::Invalid("Redirect URI 는 http://127.0.0.1:<포트>/callback 형태여야 합니다".into()));
    }
    let port = u.port().ok_or_else(|| AuthError::Invalid("Redirect URI 에 포트가 필요합니다 (예: http://127.0.0.1:8888/callback)".into()))?;
    let host = if host == "localhost" { "127.0.0.1".to_string() } else { host.to_string() };
    Ok((host, port))
}

pub fn authorize_url(client_id: &str, redirect_uri: &str, state: &str, challenge: &str) -> String {
    let mut u = url::Url::parse("https://accounts.spotify.com/authorize").unwrap();
    u.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", SCOPES)
        .append_pair("state", state)
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", challenge);
    u.to_string()
}

const DONE_PAGE: &str = r#"<!doctype html><html lang="ko"><head><meta charset="utf-8"><title>deskboard</title></head>
<body style="font-family:sans-serif;background:#121212;color:#eee;display:flex;align-items:center;justify-content:center;height:100vh">
<div style="text-align:center"><h2 style="color:#1ED760">{title}</h2><p>{message}</p></div></body></html>"#;

/// 루프백 서버를 띄우고 리다이렉트를 기다린다. 취소/타임아웃 시 즉시 포트를 놓는다.
pub fn wait_for_callback(host: &str, port: u16, expected_state: &str, cancel: Arc<AtomicBool>) -> Result<String, AuthError> {
    let server = tiny_http::Server::http((host, port))
        .map_err(|e| AuthError::Invalid(format!("{host}:{port} 를 열 수 없습니다. 다른 프로그램이 쓰고 있는지 확인하세요 ({e})")))?;
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(AuthError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(AuthError::Timeout);
        }
        let Some(req) = server.recv_timeout(Duration::from_millis(200))? else { continue };
        let url = url::Url::parse(&format!("http://x{}", req.url())).ok();
        let q = |k: &str| url.as_ref().and_then(|u| u.query_pairs().find(|(a, _)| a == k).map(|(_, v)| v.to_string()));
        let code = q("code");
        let error = q("error");
        let state = q("state");

        let (title, msg, outcome) = if let Some(code) = code {
            if state.as_deref() != Some(expected_state) {
                ("로그인 실패", "state 값이 일치하지 않습니다. 다시 시도하세요.".to_string(), Some(Err(AuthError::Invalid("state 불일치".into()))))
            } else {
                ("로그인 완료", "deskboard 로 돌아가세요. 이 창은 닫아도 됩니다.".to_string(), Some(Ok(code)))
            }
        } else if let Some(err) = error {
            ("로그인 실패", format!("승인이 거부되었습니다 ({err})."), Some(Err(AuthError::Invalid(format!("승인 거부: {err}")))))
        } else {
            ("deskboard", "인증 대기 중...".to_string(), None) // favicon 등
        };
        let body = DONE_PAGE.replace("{title}", title).replace("{message}", &msg);
        let resp = tiny_http::Response::from_string(body)
            .with_header(tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
        let _ = req.respond(resp);
        if let Some(o) = outcome {
            return o;
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: u64,
}

pub async fn exchange_code(http: &reqwest::Client, client_id: &str, redirect_uri: &str, code: &str, verifier: &str) -> Result<Token, AuthError> {
    let res = http
        .post("https://accounts.spotify.com/api/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("code_verifier", verifier),
        ])
        .send()
        .await?;
    token_from_response(res, None).await
}

pub async fn refresh(http: &reqwest::Client, client_id: &str, token: &Token) -> Result<Token, AuthError> {
    let res = http
        .post("https://accounts.spotify.com/api/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", token.refresh_token.as_str()),
            ("client_id", client_id),
        ])
        .send()
        .await?;
    token_from_response(res, Some(&token.refresh_token)).await
}

async fn token_from_response(res: reqwest::Response, fallback_refresh: Option<&str>) -> Result<Token, AuthError> {
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(AuthError::Invalid(format!("토큰 요청 실패 ({status}): {body}")));
    }
    let t: TokenResponse = res.json().await?;
    let refresh_token = t
        .refresh_token
        .or_else(|| fallback_refresh.map(str::to_string))
        .ok_or_else(|| AuthError::Invalid("refresh_token 이 없습니다".into()))?;
    Ok(Token { access_token: t.access_token, refresh_token, expires_at: now_secs() + t.expires_in })
}

// --- 저장 ---------------------------------------------------------------

pub fn auth_path(app_data: &Path) -> PathBuf {
    app_data.join("spotify.json")
}

pub fn load(path: &Path) -> AuthFile {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, f: &AuthFile) -> Result<(), AuthError> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(f).unwrap_or_default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_s256_of_verifier() {
        let (v, c) = pkce_pair();
        assert!(v.len() >= 43 && v.len() <= 128);
        assert_eq!(c, b64url(&Sha256::digest(v.as_bytes())));
        assert!(!c.contains('=') && !c.contains('+') && !c.contains('/'));
    }

    #[test]
    fn rfc7636_vector() {
        // RFC 7636 Appendix B
        let v = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(b64url(&Sha256::digest(v.as_bytes())), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn redirect_must_be_loopback() {
        assert_eq!(parse_redirect("http://127.0.0.1:8888/callback").unwrap(), ("127.0.0.1".into(), 8888));
        assert_eq!(parse_redirect("http://localhost:9000/cb").unwrap().0, "127.0.0.1");
        assert!(parse_redirect("https://example.com/callback").is_err());
        assert!(parse_redirect("http://127.0.0.1/callback").is_err());
    }

    #[test]
    fn authorize_url_has_pkce_params() {
        let u = authorize_url("cid", DEFAULT_REDIRECT_URI, "st", "ch");
        assert!(u.starts_with("https://accounts.spotify.com/authorize?"));
        assert!(u.contains("code_challenge_method=S256"));
        assert!(u.contains("code_challenge=ch"));
        assert!(u.contains("state=st"));
        assert!(u.contains("client_id=cid"));
    }

    #[test]
    fn callback_server_validates_state_and_cancels() {
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let h = std::thread::spawn(move || wait_for_callback("127.0.0.1", 18999, "abc", c2));
        std::thread::sleep(Duration::from_millis(300));
        let http = reqwest::blocking::Client::new();
        let _ = http.get("http://127.0.0.1:18999/callback?code=XYZ&state=abc").send().unwrap();
        assert_eq!(h.join().unwrap().unwrap(), "XYZ");

        let c3 = cancel.clone();
        let h = std::thread::spawn(move || wait_for_callback("127.0.0.1", 18999, "abc", c3));
        std::thread::sleep(Duration::from_millis(100));
        cancel.store(true, Ordering::Relaxed);
        assert!(matches!(h.join().unwrap(), Err(AuthError::Cancelled)));
    }
}
