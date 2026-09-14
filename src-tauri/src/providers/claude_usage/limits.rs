//! 구독 요금제 한도(5시간 / 주간) — Claude Code 가 `/usage` 에 쓰는 것과 같은 OAuth 엔드포인트.
//!
//! 토큰은 Claude Code 가 저장한 `~/.claude/.credentials.json` 의 `claudeAiOauth.accessToken` 을 읽기만 한다
//! (갱신은 Claude Code 에 맡긴다 — 우리가 refresh 하면 Claude Code 쪽 토큰이 깨질 수 있다).
//! 비공식 엔드포인트라 응답 형식은 방어적으로 파싱한다.

use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// Claude Code 가 쓰는 공개 OAuth 클라이언트 (PKCE, secret 없음). 토큰 갱신에만 쓴다.
const TOKEN_URLS: [&str; 2] = ["https://platform.claude.com/v1/oauth/token", "https://console.anthropic.com/v1/oauth/token"];
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct LimitWindow {
    /// 0~100 (%)
    pub utilization: f64,
    /// ISO-8601, 없을 수 있음
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Limits {
    pub ok: bool,
    pub error: Option<String>,
    pub subscription: Option<String>,
    pub five_hour: Option<LimitWindow>,
    pub seven_day: Option<LimitWindow>,
    pub seven_day_opus: Option<LimitWindow>,
    pub seven_day_sonnet: Option<LimitWindow>,
    /// unix ms
    pub fetched_at: u64,
}

pub fn credentials_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude").join(".credentials.json")
}

pub struct Credentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub subscription: Option<String>,
    /// unix ms
    pub expires_at: Option<u64>,
}

pub fn read_credentials(path: &std::path::Path) -> Result<Credentials, String> {
    let text = std::fs::read_to_string(path).map_err(|_| "Claude Code 로그인 정보가 없습니다 (claude 를 한 번 실행하세요)".to_string())?;
    let v: Value = serde_json::from_str(&text).map_err(|_| "credentials 파일 형식 오류".to_string())?;
    let o = v.get("claudeAiOauth").ok_or("claude.ai OAuth 로그인이 아닙니다 (API 키 방식은 한도 조회 불가)")?;
    let access_token = o.get("accessToken").and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or("accessToken 없음")?.to_string();
    Ok(Credentials {
        access_token,
        refresh_token: o.get("refreshToken").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string),
        subscription: o.get("subscriptionType").and_then(Value::as_str).map(str::to_string),
        expires_at: o.get("expiresAt").and_then(Value::as_u64),
    })
}

/// 만료된 access token 을 refresh token 으로 갱신하고, Claude Code 가 읽는 파일에 같은 형식으로 되쓴다.
/// (Claude Code CLI 도 같은 방식으로 갱신·저장하므로 서로 호환된다. 다른 필드는 보존.)
async fn refresh_and_store(http: &reqwest::Client, path: &std::path::Path, refresh_token: &str) -> Result<String, String> {
    let payload = serde_json::json!({ "grant_type": "refresh_token", "refresh_token": refresh_token, "client_id": CLIENT_ID });
    let mut last_err = String::new();
    let mut body: Option<Value> = None;
    for url in TOKEN_URLS {
        let res = http.post(url).json(&payload).send().await.map_err(|e| format!("네트워크 오류: {e}"))?;
        let status = res.status();
        let v: Value = res.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            body = Some(v);
            break;
        }
        let detail = v.get("error").and_then(Value::as_str).or_else(|| v.get("error_description").and_then(Value::as_str)).unwrap_or("").to_string();
        last_err = format!("토큰 갱신 실패 ({status}{})", if detail.is_empty() { String::new() } else { format!(": {detail}") });
        log::warn!("claude oauth refresh via {url}: {last_err}");
        // 4xx 중 invalid_grant(리프레시 토큰 폐기)는 다른 엔드포인트도 같으므로 중단
        if detail.contains("invalid_grant") { break; }
    }
    let body = body.ok_or(last_err)?;
    let access = body.get("access_token").and_then(Value::as_str).ok_or("access_token 없음")?.to_string();
    let new_refresh = body.get("refresh_token").and_then(Value::as_str).unwrap_or(refresh_token).to_string();
    let expires_in = body.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);

    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut file: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if let Some(o) = file.get_mut("claudeAiOauth").and_then(Value::as_object_mut) {
        o.insert("accessToken".into(), Value::String(access.clone()));
        o.insert("refreshToken".into(), Value::String(new_refresh));
        o.insert("expiresAt".into(), Value::from(now_ms() + expires_in * 1000));
    }
    // 원자적 교체: 임시 파일에 쓰고 rename
    let tmp = path.with_extension("json.deskboard-tmp");
    std::fs::write(&tmp, serde_json::to_string(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    log::info!("claude oauth token refreshed");
    Ok(access)
}

fn window_from(v: Option<&Value>) -> Option<LimitWindow> {
    let v = v?;
    if v.is_null() { return None; }
    let util = v.get("utilization").and_then(Value::as_f64)?;
    // 0~1 로 오면 % 로 환산
    let utilization = if util <= 1.0 && v.get("utilization").map_or(false, |u| u.is_f64()) && util < 1.0 { util * 100.0 } else { util };
    Some(LimitWindow { utilization, resets_at: v.get("resets_at").and_then(Value::as_str).map(str::to_string) })
}

pub fn parse_usage(v: &Value) -> Limits {
    Limits {
        ok: true,
        error: None,
        subscription: None,
        five_hour: window_from(v.get("five_hour")),
        seven_day: window_from(v.get("seven_day")),
        seven_day_opus: window_from(v.get("seven_day_opus")),
        seven_day_sonnet: window_from(v.get("seven_day_sonnet")),
        fetched_at: now_ms(),
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub async fn fetch(http: &reqwest::Client, path: &std::path::Path) -> Limits {
    let fail = |e: String| Limits { ok: false, error: Some(e), fetched_at: now_ms(), ..Default::default() };
    let mut creds = match read_credentials(path) {
        Ok(c) => c,
        Err(e) => return fail(e),
    };
    // 초 단위 expiresAt 도 허용. 만료(임박)면 먼저 갱신, 그래도 401 이면 한 번 더 갱신 시도.
    let expires_ms = creds.expires_at.map(|t| if t < 10_000_000_000 { t * 1000 } else { t });
    let stale = expires_ms.map_or(false, |t| t < now_ms() + 60_000);
    let mut refreshed = false;
    if stale {
        match &creds.refresh_token {
            Some(rt) => match refresh_and_store(http, path, rt).await {
                Ok(a) => { creds.access_token = a; refreshed = true; }
                Err(e) if e.contains("invalid_grant") => return fail("로그인 만료 — 터미널에서 claude 실행 후 /login 하면 자동 연결됩니다".into()),
                Err(e) => return fail(format!("{e} — Claude Code 에서 /login 으로 다시 로그인하세요")),
            },
            None => return fail("토큰이 만료되었고 refresh token 이 없습니다. Claude Code 에서 /login 하세요".into()),
        }
    }
    let call = |token: String| async move {
        http.get(USAGE_URL)
            .bearer_auth(token)
            .header("anthropic-beta", "oauth-2025-04-20")
            .header("User-Agent", "deskboard")
            .send()
            .await
    };
    let mut res = match call(creds.access_token.clone()).await {
        Ok(r) => r,
        Err(e) => return fail(format!("네트워크 오류: {e}")),
    };
    if res.status().as_u16() == 401 && !refreshed {
        if let Some(rt) = &creds.refresh_token {
            if let Ok(a) = refresh_and_store(http, path, rt).await {
                res = match call(a).await { Ok(r) => r, Err(e) => return fail(format!("네트워크 오류: {e}")) };
            }
        }
    }
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    if !status.is_success() {
        let msg = match status.as_u16() {
            401 | 403 => "인증 실패 — Claude Code 에서 /login 으로 다시 로그인하세요".to_string(),
            429 => "요청이 너무 많습니다".to_string(),
            c => format!("한도 조회 실패 ({c})"),
        };
        return fail(msg);
    }
    let v: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return fail("응답 형식 오류".into()),
    };
    if let Some(obj) = v.as_object() {
        log::info!("usage response keys: {:?}", obj.keys().collect::<Vec<_>>());
    }
    let mut l = parse_usage(&v);
    l.subscription = creds.subscription;
    if l.five_hour.is_none() && l.seven_day.is_none() {
        l.ok = false;
        l.error = Some(format!("알 수 없는 응답 형식: {}", v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>().join(",")).unwrap_or_default()));
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_percent_windows() {
        let v = json!({
            "five_hour": {"utilization": 12.5, "resets_at": "2026-09-14T15:00:00Z"},
            "seven_day": {"utilization": 40, "resets_at": "2026-09-20T00:00:00Z"},
            "seven_day_opus": null
        });
        let l = parse_usage(&v);
        assert!(l.ok);
        assert_eq!(l.five_hour.as_ref().unwrap().utilization, 12.5);
        assert_eq!(l.seven_day.as_ref().unwrap().utilization, 40.0);
        assert!(l.seven_day_opus.is_none());
        assert_eq!(l.five_hour.unwrap().resets_at.as_deref(), Some("2026-09-14T15:00:00Z"));
    }

    #[test]
    fn fraction_becomes_percent() {
        let l = parse_usage(&json!({"five_hour": {"utilization": 0.25}}));
        assert_eq!(l.five_hour.unwrap().utilization, 25.0);
    }

    #[test]
    fn reads_credentials_file() {
        let dir = std::env::temp_dir().join(format!("deskboard-cred-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(".credentials.json");
        std::fs::write(&p, r#"{"claudeAiOauth":{"accessToken":"tok","subscriptionType":"max","expiresAt":9999999999999}}"#).unwrap();
        let c = read_credentials(&p).unwrap();
        assert_eq!(c.access_token, "tok");
        assert_eq!(c.subscription.as_deref(), Some("max"));
        std::fs::write(&p, r#"{"apiKey":"x"}"#).unwrap();
        assert!(read_credentials(&p).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
