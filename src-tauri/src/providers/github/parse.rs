//! GitHub 응답에서 필요한 값만 뽑는다. 순수 함수라 고정 JSON 으로 테스트한다.

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Check {
    /// "owner/repo"
    pub repo: String,
    /// 워크플로 이름
    pub name: String,
    /// "success" | "failure" | "cancelled" | "running" | "unknown"
    pub state: String,
    pub branch: String,
}

#[derive(Debug, Deserialize)]
struct SearchResult {
    total_count: u32,
}

/// 검색 API 의 건수.
pub fn parse_count(json: &str) -> Result<u32, String> {
    serde_json::from_str::<SearchResult>(json)
        .map(|r| r.total_count)
        .map_err(|e| format!("GitHub 검색 결과를 읽지 못했습니다: {e}"))
}

#[derive(Debug, Deserialize)]
struct Notification {
    unread: Option<bool>,
}

/// 미확인 알림 수. `/notifications` 는 기본이 미확인만이라 대개 길이와 같지만,
/// `unread` 가 명시돼 있으면 그걸 센다.
pub fn parse_notifications(json: &str) -> Result<u32, String> {
    let list: Vec<Notification> = serde_json::from_str(json)
        .map_err(|e| format!("GitHub 알림을 읽지 못했습니다: {e}"))?;
    Ok(list.iter().filter(|n| n.unread.unwrap_or(true)).count() as u32)
}

#[derive(Debug, Deserialize)]
struct RunsResponse {
    workflow_runs: Vec<Run>,
}

#[derive(Debug, Deserialize)]
struct Run {
    name: Option<String>,
    head_branch: Option<String>,
    /// "queued" | "in_progress" | "completed"
    status: Option<String>,
    /// completed 일 때만: "success" | "failure" | "cancelled" | "skipped" …
    conclusion: Option<String>,
}

/// 최신 workflow 실행 결과.
pub fn parse_checks(json: &str, repo: &str) -> Result<Vec<Check>, String> {
    let r: RunsResponse = serde_json::from_str(json)
        .map_err(|e| format!("GitHub 실행 결과를 읽지 못했습니다: {e}"))?;
    Ok(r.workflow_runs
        .into_iter()
        .map(|run| Check {
            repo: repo.to_string(),
            name: run.name.unwrap_or_else(|| "workflow".into()),
            state: run_state(run.status.as_deref(), run.conclusion.as_deref()).to_string(),
            branch: run.head_branch.unwrap_or_default(),
        })
        .collect())
}

/// 상태 + 결론 → 위젯이 색을 고를 한 단어.
///
/// 아직 돌고 있으면 결론이 없다 — 그걸 실패로 칠하면 안 된다.
pub fn run_state(status: Option<&str>, conclusion: Option<&str>) -> &'static str {
    match status {
        Some("queued") | Some("in_progress") | Some("waiting") | Some("requested") | Some("pending") => "running",
        _ => match conclusion {
            Some("success") => "success",
            Some("failure") | Some("timed_out") | Some("startup_failure") => "failure",
            Some("cancelled") => "cancelled",
            Some("skipped") | Some("neutral") => "skipped",
            Some(_) => "unknown",
            None => "unknown",
        },
    }
}

/// 실패·한도 소진 뒤 얼마나 쉴지. 정상 주기의 두 배로 물러선다.
pub fn rate_limit_backoff(normal: Duration) -> Duration {
    // 15분을 넘기면 "고장난 위젯"처럼 보이므로 상한을 둔다.
    (normal * 2).min(Duration::from_secs(15 * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_counts_are_read() {
        assert_eq!(parse_count(r#"{"total_count":3,"items":[]}"#).unwrap(), 3);
        assert_eq!(parse_count(r#"{"total_count":0,"items":[]}"#).unwrap(), 0);
    }

    #[test]
    fn a_broken_search_response_reports_a_readable_error() {
        assert!(parse_count("nope").unwrap_err().contains("읽지 못했습니다"));
        assert!(parse_count("{}").is_err());
    }

    #[test]
    fn notifications_are_counted() {
        assert_eq!(parse_notifications(r#"[{"unread":true},{"unread":true}]"#).unwrap(), 2);
        assert_eq!(parse_notifications("[]").unwrap(), 0);
    }

    #[test]
    fn read_notifications_are_not_counted() {
        assert_eq!(parse_notifications(r#"[{"unread":true},{"unread":false}]"#).unwrap(), 1);
    }

    #[test]
    fn a_missing_unread_flag_counts_as_unread() {
        // /notifications 는 기본이 미확인만 준다
        assert_eq!(parse_notifications(r#"[{},{}]"#).unwrap(), 2);
    }

    #[test]
    fn the_latest_run_becomes_a_check() {
        let json = r#"{"workflow_runs":[
          {"name":"CI","head_branch":"main","status":"completed","conclusion":"success"}
        ]}"#;
        let c = parse_checks(json, "owner/repo").unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0], Check {
            repo: "owner/repo".into(), name: "CI".into(),
            state: "success".into(), branch: "main".into(),
        });
    }

    #[test]
    fn a_repository_with_no_runs_yields_nothing() {
        assert_eq!(parse_checks(r#"{"workflow_runs":[]}"#, "a/b").unwrap().len(), 0);
    }

    #[test]
    fn a_run_still_going_is_running_not_failed() {
        // 결론이 없다고 실패로 칠하면 빨간 점이 상시로 뜬다
        assert_eq!(run_state(Some("in_progress"), None), "running");
        assert_eq!(run_state(Some("queued"), None), "running");
        assert_eq!(run_state(Some("in_progress"), Some("failure")), "running");
    }

    #[test]
    fn completed_runs_map_to_their_conclusion() {
        assert_eq!(run_state(Some("completed"), Some("success")), "success");
        assert_eq!(run_state(Some("completed"), Some("failure")), "failure");
        assert_eq!(run_state(Some("completed"), Some("timed_out")), "failure");
        assert_eq!(run_state(Some("completed"), Some("cancelled")), "cancelled");
        assert_eq!(run_state(Some("completed"), Some("skipped")), "skipped");
    }

    #[test]
    fn unknown_states_do_not_masquerade_as_success() {
        assert_eq!(run_state(Some("completed"), Some("weird")), "unknown");
        assert_eq!(run_state(None, None), "unknown");
        assert_eq!(run_state(Some("completed"), None), "unknown");
    }

    #[test]
    fn a_run_without_a_name_still_shows_something() {
        let json = r#"{"workflow_runs":[{"status":"completed","conclusion":"success"}]}"#;
        let c = parse_checks(json, "a/b").unwrap();
        assert_eq!(c[0].name, "workflow");
        assert_eq!(c[0].branch, "");
    }

    #[test]
    fn backoff_doubles_but_is_capped() {
        assert_eq!(rate_limit_backoff(Duration::from_secs(300)), Duration::from_secs(600));
        assert_eq!(rate_limit_backoff(Duration::from_secs(600)), Duration::from_secs(900));
        // 상한을 넘지 않는다 — 넘으면 고장난 것처럼 보인다
        assert_eq!(rate_limit_backoff(Duration::from_secs(3600)), Duration::from_secs(900));
    }
}
