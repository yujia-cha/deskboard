//! 구독 요금제 한도(5시간 / 주간) — Claude Code 가 `/usage` 에 쓰는 것과 같은 OAuth 엔드포인트.
//! 토큰은 `auth.rs` (deskboard 전용 파일). 비공식 엔드포인트라 응답 형식은 방어적으로 파싱한다.

use super::auth::{self, Token};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

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
    pub logged_in: bool,
    pub error: Option<String>,
    pub five_hour: Option<LimitWindow>,
    pub seven_day: Option<LimitWindow>,
    pub seven_day_opus: Option<LimitWindow>,
    pub seven_day_sonnet: Option<LimitWindow>,
    /// unix ms
    pub fetched_at: u64,
}

fn window_from(v: Option<&Value>) -> Option<LimitWindow> {
    let v = v?;
    if v.is_null() { return None; }
    // API 는 % 단위 (0~100). 0.8 은 0.8% 이지 80% 가 아니다.
    let utilization = v.get("utilization").and_then(Value::as_f64)?.clamp(0.0, 100.0);
    Some(LimitWindow { utilization, resets_at: v.get("resets_at").and_then(Value::as_str).map(str::to_string) })
}

pub fn parse_usage(v: &Value) -> Limits {
    Limits {
        ok: true,
        logged_in: true,
        error: None,
        five_hour: window_from(v.get("five_hour")),
        seven_day: window_from(v.get("seven_day")),
        seven_day_opus: window_from(v.get("seven_day_opus")),
        seven_day_sonnet: window_from(v.get("seven_day_sonnet")),
        fetched_at: auth::now_ms(),
    }
}

pub async fn fetch(http: &reqwest::Client, token_file: &Path) -> Limits {
    let fail = |e: String| Limits { ok: false, logged_in: true, error: Some(e), fetched_at: auth::now_ms(), ..Default::default() };
    let Some(mut token) = auth::load(token_file) else {
        return Limits { ok: false, logged_in: false, fetched_at: auth::now_ms(), ..Default::default() };
    };
    let mut refreshed = false;
    if token.expires_at < auth::now_ms() + 60_000 {
        match auth::refresh(http, token_file, &token).await {
            Ok(t) => { token = t; refreshed = true; }
            Err(e) => return fail(format!("{e} — 다시 로그인하세요")),
        }
    }
    let call = |t: &Token| {
        http.get(USAGE_URL)
            .bearer_auth(t.access_token.clone())
            .header("anthropic-beta", "oauth-2025-04-20")
            .header("User-Agent", "deskboard")
            .send()
    };
    let mut res = match call(&token).await {
        Ok(r) => r,
        Err(e) => return fail(format!("네트워크 오류: {e}")),
    };
    if res.status().as_u16() == 401 && !refreshed {
        if let Ok(t) = auth::refresh(http, token_file, &token).await {
            res = match call(&t).await { Ok(r) => r, Err(e) => return fail(format!("네트워크 오류: {e}")) };
        }
    }
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    if !status.is_success() {
        let msg = match status.as_u16() {
            401 | 403 => "인증 실패 — 다시 로그인하세요".to_string(),
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
        assert!(l.ok && l.logged_in);
        assert_eq!(l.five_hour.as_ref().unwrap().utilization, 12.5);
        assert_eq!(l.seven_day.as_ref().unwrap().utilization, 40.0);
        assert!(l.seven_day_opus.is_none());
        assert_eq!(l.five_hour.unwrap().resets_at.as_deref(), Some("2026-09-14T15:00:00Z"));
    }

    #[test]
    fn small_percent_is_not_rescaled() {
        let l = parse_usage(&json!({"five_hour": {"utilization": 0.8}}));
        assert_eq!(l.five_hour.unwrap().utilization, 0.8);
        let l = parse_usage(&json!({"five_hour": {"utilization": 250}}));
        assert_eq!(l.five_hour.unwrap().utilization, 100.0);
    }

    #[tokio::test]
    async fn not_logged_in_without_token_file() {
        let l = fetch(&reqwest::Client::new(), Path::new("Z:/definitely/missing/claude.json")).await;
        assert!(!l.ok && !l.logged_in && l.error.is_none());
    }
}
