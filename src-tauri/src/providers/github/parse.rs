//! GitHub 응답에서 필요한 값만 뽑는다. 순수 함수라 고정 JSON 으로 테스트한다.
//!
//! 본체는 **GraphQL 응답 하나**다 (`parse_graphql`) — 기여도 잔디, 리뷰 요청·담당 이슈,
//! 저장소별 열린 PR/이슈와 기본 브랜치 CI 를 한 번에 받는다. REST 로 하면 저장소마다
//! 호출이 하나씩 늘어나는데, GraphQL 은 저장소를 별칭(`r0:`, `r1:` …)으로 나란히 붙이면 한 번이다.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// 기여도 달력의 하루.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Day {
    /// "YYYY-MM-DD"
    pub date: String,
    pub count: u32,
    /// 0(없음)~4. 위젯이 색 농도로 쓴다.
    pub level: u8,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Contributions {
    pub total: u32,
    /// 주 단위 묶음. 각 주는 일요일부터지만 첫 주·마지막 주는 잘려 있을 수 있다.
    pub weeks: Vec<Vec<Day>>,
}

/// 저장소 한 줄 — "내가 지금 봐야 할 것"을 한눈에.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct RepoStat {
    /// "owner/repo"
    pub repo: String,
    pub open_prs: u32,
    /// 그중 내 리뷰를 기다리는 것 (강조 대상)
    pub review_prs: u32,
    pub open_issues: u32,
    /// 나에게 배정된 열린 이슈
    pub assigned_issues: u32,
    /// 이 저장소의 미확인 알림
    pub notifications: u32,
    /// 기본 브랜치 CI: "success" | "failure" | "running" | "unknown" | "none"
    pub ci: String,
    pub branch: String,
}

/// GraphQL 한 번으로 받아오는 것 전부.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphData {
    pub login: String,
    pub contributions: Option<Contributions>,
    pub review_requests: u32,
    pub assigned_issues: u32,
    /// 저장소별 리뷰 요청 수 — 설정에 없는 저장소도 들어온다.
    pub review_by_repo: HashMap<String, u32>,
    pub assigned_by_repo: HashMap<String, u32>,
    pub repos: Vec<RepoStat>,
    /// GraphQL 이 부분 실패를 돌려줬을 때의 사람 읽을 메시지.
    pub error: Option<String>,
}

// --- 질의 ---------------------------------------------------------------------------

/// `owner/repo` 를 GraphQL 에 넣어도 안전한 조각으로 나눈다.
///
/// 이름을 질의문에 그대로 끼워 넣으므로 **GitHub 이름에 실제로 쓰이는 글자만** 통과시킨다.
/// 값은 변수로 넘기는 편이 낫겠지만 저장소마다 별칭이 달라 질의문 자체를 만들어야 한다.
pub fn split_repo(s: &str) -> Option<(String, String)> {
    let s = s.trim().trim_matches('/');
    let (owner, name) = s.split_once('/')?;
    let ok = |p: &str| {
        !p.is_empty()
            && p.len() <= 100
            && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (ok(owner) && ok(name)).then(|| (owner.to_string(), name.to_string()))
}

/// 설정 문자열에서 실제로 물어볼 저장소 목록을 만든다.
///
/// **중복을 없애야 한다** — 같은 이름을 두 번 적으면 질의에 `r0`·`r1` 두 별칭이 생기고
/// 위젯에도 같은 줄이 두 번 나온다 (프론트는 저장소 이름을 key 로 쓴다). 대소문자만 다른
/// 것도 GitHub 에서는 같은 저장소다.
pub fn normalize_repos(raw: impl IntoIterator<Item = String>, max: usize) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for r in raw {
        let Some((owner, name)) = split_repo(&r) else { continue };
        let full = format!("{owner}/{name}");
        let key = full.to_ascii_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(full);
        if out.len() >= max {
            break;
        }
    }
    out
}

/// 기여도 + 리뷰/이슈 검색 + 저장소별 통계를 한 번에 묻는 질의문.
pub fn build_query(repos: &[String]) -> String {
    let mut q = String::from(
        r#"query {
  viewer {
    login
    contributionsCollection {
      contributionCalendar {
        totalContributions
        weeks { contributionDays { date contributionCount } }
      }
    }
  }
  reviews: search(query: "is:open is:pr review-requested:@me", type: ISSUE, first: 50) {
    issueCount
    nodes { ... on PullRequest { repository { nameWithOwner } } }
  }
  assigned: search(query: "is:open is:issue assignee:@me", type: ISSUE, first: 50) {
    issueCount
    nodes { ... on Issue { repository { nameWithOwner } } }
  }
"#,
    );
    for (i, (owner, name)) in repos.iter().filter_map(|r| split_repo(r)).enumerate() {
        q.push_str(&format!(
            "  r{i}: repository(owner: \"{owner}\", name: \"{name}\") {{ nameWithOwner \
             pullRequests(states: OPEN) {{ totalCount }} issues(states: OPEN) {{ totalCount }} \
             defaultBranchRef {{ name target {{ ... on Commit {{ statusCheckRollup {{ state }} }} }} }} }}\n"
        ));
    }
    q.push_str("}\n");
    q
}

// --- 응답 ---------------------------------------------------------------------------

/// `statusCheckRollup.state` → 위젯이 색을 고를 한 단어.
///
/// 아직 돌고 있는 것(PENDING/EXPECTED)을 실패로 칠하면 빨간 점이 상시로 뜬다.
pub fn rollup_state(state: Option<&str>) -> &'static str {
    match state {
        Some("SUCCESS") => "success",
        Some("FAILURE") | Some("ERROR") => "failure",
        Some("PENDING") | Some("EXPECTED") => "running",
        Some(_) => "unknown",
        None => "none", // CI 가 아예 없는 저장소 — 회색 점도 띄우지 않는다
    }
}

fn level_of(count: u32, max: u32) -> u8 {
    if count == 0 || max == 0 {
        return 0;
    }
    // GitHub 처럼 4단계. 최댓값 대비 비율이라 활동량이 적은 계정에서도 대비가 산다.
    let r = f64::from(count) / f64::from(max);
    match r {
        r if r > 0.75 => 4,
        r if r > 0.5 => 3,
        r if r > 0.25 => 2,
        _ => 1,
    }
}

#[derive(Debug, Deserialize)]
struct GqlEnvelope {
    data: Option<serde_json::Value>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Debug, Deserialize)]
struct GqlError {
    message: String,
}

/// GraphQL 응답을 통째로 읽는다.
///
/// **부분 실패를 살려야 한다** — 저장소 이름에 오타가 있으면 그 필드만 null 이고
/// `errors` 에 한 줄이 들어온다. 전체를 오류로 처리하면 잔디까지 사라진다.
pub fn parse_graphql(json: &str, repos: &[String]) -> Result<GraphData, String> {
    let env: GqlEnvelope =
        serde_json::from_str(json).map_err(|e| format!("GitHub 응답을 읽지 못했습니다: {e}"))?;
    let Some(data) = env.data else {
        let msg = env.errors.first().map_or_else(|| "빈 응답".to_string(), |e| e.message.clone());
        return Err(format!("GitHub: {msg}"));
    };

    let mut out = GraphData {
        login: data.pointer("/viewer/login").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        ..Default::default()
    };
    if !env.errors.is_empty() {
        out.error = Some(env.errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join(" · "));
    }

    // 기여도 달력
    if let Some(cal) = data.pointer("/viewer/contributionsCollection/contributionCalendar") {
        let total = cal.get("totalContributions").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
        let mut weeks: Vec<Vec<Day>> = Vec::new();
        let mut max = 0;
        for w in cal.get("weeks").and_then(|v| v.as_array()).map_or(&[][..], |v| v.as_slice()) {
            let mut days = Vec::new();
            for d in w.get("contributionDays").and_then(|v| v.as_array()).map_or(&[][..], |v| v.as_slice()) {
                let count = d.get("contributionCount").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
                max = max.max(count);
                days.push(Day {
                    date: d.get("date").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    count,
                    level: 0,
                });
            }
            weeks.push(days);
        }
        // 단계는 최댓값을 알아야 매겨진다 → 한 바퀴 더 돈다
        for d in weeks.iter_mut().flatten() {
            d.level = level_of(d.count, max);
        }
        out.contributions = Some(Contributions { total, weeks });
    }

    let search = |key: &str| -> (u32, HashMap<String, u32>) {
        let node = data.get(key);
        let count = node
            .and_then(|v| v.get("issueCount"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        let mut by = HashMap::new();
        for n in node
            .and_then(|v| v.get("nodes"))
            .and_then(|v| v.as_array())
            .map_or(&[][..], |v| v.as_slice())
        {
            if let Some(r) = n.pointer("/repository/nameWithOwner").and_then(|v| v.as_str()) {
                *by.entry(r.to_string()).or_insert(0) += 1;
            }
        }
        (count, by)
    };
    (out.review_requests, out.review_by_repo) = search("reviews");
    (out.assigned_issues, out.assigned_by_repo) = search("assigned");

    // 저장소 별칭 — 질의문에서 건너뛴 이름(형식 위반)은 여기서도 빠진다
    for i in 0..repos.iter().filter_map(|r| split_repo(r)).count() {
        let Some(r) = data.get(format!("r{i}")).filter(|v| !v.is_null()) else { continue };
        let repo = r.get("nameWithOwner").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if repo.is_empty() {
            continue;
        }
        out.repos.push(RepoStat {
            open_prs: r.pointer("/pullRequests/totalCount").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32,
            open_issues: r.pointer("/issues/totalCount").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32,
            review_prs: *out.review_by_repo.get(&repo).unwrap_or(&0),
            assigned_issues: *out.assigned_by_repo.get(&repo).unwrap_or(&0),
            notifications: 0, // 알림은 REST 쪽에서 채운다
            ci: rollup_state(
                r.pointer("/defaultBranchRef/target/statusCheckRollup/state").and_then(|v| v.as_str()),
            )
            .to_string(),
            branch: r.pointer("/defaultBranchRef/name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            repo,
        });
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct Notification {
    unread: Option<bool>,
    repository: Option<NotificationRepo>,
}

#[derive(Debug, Deserialize)]
struct NotificationRepo {
    full_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Notifications {
    pub total: u32,
    pub by_repo: HashMap<String, u32>,
}

/// 미확인 알림 — 전체 개수와 저장소별 개수.
/// `/notifications` 는 기본이 미확인만이라 대개 길이와 같지만, `unread` 가 있으면 그걸 센다.
pub fn parse_notifications(json: &str) -> Result<Notifications, String> {
    let list: Vec<Notification> =
        serde_json::from_str(json).map_err(|e| format!("GitHub 알림을 읽지 못했습니다: {e}"))?;
    let mut out = Notifications::default();
    for n in list.iter().filter(|n| n.unread.unwrap_or(true)) {
        out.total += 1;
        if let Some(full) = n.repository.as_ref().and_then(|r| r.full_name.clone()) {
            *out.by_repo.entry(full).or_insert(0) += 1;
        }
    }
    Ok(out)
}

/// 실패·한도 소진 뒤 얼마나 쉴지. 정상 주기의 두 배로 물러선다.
pub fn rate_limit_backoff(normal: Duration) -> Duration {
    // 15분을 넘기면 "고장난 위젯"처럼 보이므로 상한을 둔다.
    (normal * 2).min(Duration::from_secs(15 * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GQL: &str = r#"{"data":{
      "viewer":{"login":"me","contributionsCollection":{"contributionCalendar":{
        "totalContributions":7,
        "weeks":[{"contributionDays":[
          {"date":"2026-01-04","contributionCount":0},
          {"date":"2026-01-05","contributionCount":1},
          {"date":"2026-01-06","contributionCount":4}]}]}}},
      "reviews":{"issueCount":2,"nodes":[
        {"repository":{"nameWithOwner":"a/b"}},{"repository":{"nameWithOwner":"a/b"}}]},
      "assigned":{"issueCount":1,"nodes":[{"repository":{"nameWithOwner":"c/d"}}]},
      "r0":{"nameWithOwner":"a/b","pullRequests":{"totalCount":5},"issues":{"totalCount":3},
        "defaultBranchRef":{"name":"main","target":{"statusCheckRollup":{"state":"SUCCESS"}}}}
    }}"#;

    #[test]
    fn a_full_response_becomes_one_snapshot() {
        let g = parse_graphql(GQL, &["a/b".into()]).unwrap();
        assert_eq!(g.login, "me");
        assert_eq!(g.review_requests, 2);
        assert_eq!(g.assigned_issues, 1);
        let c = g.contributions.clone().unwrap();
        assert_eq!(c.total, 7);
        assert_eq!(c.weeks[0].len(), 3);
        assert_eq!(g.repos.len(), 1);
        assert_eq!(g.repos[0].repo, "a/b");
        assert_eq!(g.repos[0].open_prs, 5);
        assert_eq!(g.repos[0].open_issues, 3);
        assert_eq!(g.repos[0].review_prs, 2); // 검색 결과를 저장소 줄에 이어 붙인다
        assert_eq!(g.repos[0].ci, "success");
        assert_eq!(g.repos[0].branch, "main");
    }

    #[test]
    fn contribution_levels_scale_to_the_busiest_day() {
        let g = parse_graphql(GQL, &[]).unwrap();
        let days = g.contributions.unwrap().weeks[0].clone();
        assert_eq!(days[0].level, 0); // 0건은 언제나 0단계
        assert_eq!(days[2].level, 4); // 최댓값은 4단계
        assert!(days[1].level >= 1 && days[1].level < 4);
    }

    #[test]
    fn a_partial_failure_keeps_what_did_arrive() {
        // 저장소 이름 오타 → 그 필드만 null 이고 errors 에 한 줄. 잔디까지 버리면 안 된다.
        let json = r#"{"data":{"viewer":{"login":"me"},"reviews":{"issueCount":1,"nodes":[]},
          "assigned":{"issueCount":0,"nodes":[]},"r0":null},
          "errors":[{"message":"Could not resolve to a Repository"}]}"#;
        let g = parse_graphql(json, &["a/typo".into()]).unwrap();
        assert_eq!(g.login, "me");
        assert_eq!(g.review_requests, 1);
        assert!(g.repos.is_empty());
        assert!(g.error.unwrap().contains("Could not resolve"));
    }

    #[test]
    fn a_response_without_data_is_an_error() {
        let e = parse_graphql(r#"{"errors":[{"message":"Bad credentials"}]}"#, &[]).unwrap_err();
        assert!(e.contains("Bad credentials"));
        assert!(parse_graphql("nope", &[]).is_err());
    }

    #[test]
    fn repositories_line_up_with_their_aliases() {
        // 형식이 틀린 이름은 질의문에서 빠지므로 별칭 번호가 밀린다 — 양쪽이 같은 규칙을 써야 한다
        let json = r#"{"data":{"viewer":{"login":"me"},"reviews":{"issueCount":0,"nodes":[]},
          "assigned":{"issueCount":0,"nodes":[]},
          "r0":{"nameWithOwner":"a/b","pullRequests":{"totalCount":1},"issues":{"totalCount":0},"defaultBranchRef":null}}}"#;
        let g = parse_graphql(json, &["bad-name".into(), "a/b".into()]).unwrap();
        assert_eq!(g.repos.len(), 1);
        assert_eq!(g.repos[0].repo, "a/b");
        assert_eq!(g.repos[0].ci, "none"); // CI 가 없는 저장소는 점을 띄우지 않는다
    }

    #[test]
    fn the_query_only_contains_safe_repository_names() {
        let q = build_query(&["owner/repo".into(), "a\"); evil {".into(), "no-slash".into()]);
        assert!(q.contains("r0: repository(owner: \"owner\", name: \"repo\")"));
        assert!(!q.contains("evil"));
        assert!(!q.contains("r1:"));
    }

    #[test]
    fn the_repository_list_drops_duplicates_and_junk() {
        let got = normalize_repos(
            ["a/b".into(), " a/b ".into(), "A/B".into(), "no-slash".into(), "c/d".into()],
            8,
        );
        assert_eq!(got, vec!["a/b".to_string(), "c/d".to_string()]);
    }

    #[test]
    fn the_repository_list_is_capped() {
        let many: Vec<String> = (0..20).map(|i| format!("o/r{i}")).collect();
        assert_eq!(normalize_repos(many, 3).len(), 3);
    }

    #[test]
    fn repository_names_are_split_and_validated() {
        assert_eq!(split_repo(" owner/repo "), Some(("owner".into(), "repo".into())));
        assert_eq!(split_repo("owner/repo.js"), Some(("owner".into(), "repo.js".into())));
        assert_eq!(split_repo("owner"), None);
        assert_eq!(split_repo("owner/"), None);
        assert_eq!(split_repo("own er/repo"), None);
    }

    #[test]
    fn running_checks_are_not_failures() {
        assert_eq!(rollup_state(Some("PENDING")), "running");
        assert_eq!(rollup_state(Some("EXPECTED")), "running");
        assert_eq!(rollup_state(Some("FAILURE")), "failure");
        assert_eq!(rollup_state(Some("ERROR")), "failure");
        assert_eq!(rollup_state(Some("SUCCESS")), "success");
        assert_eq!(rollup_state(Some("WEIRD")), "unknown");
        assert_eq!(rollup_state(None), "none");
    }

    #[test]
    fn notifications_are_counted_per_repository() {
        let json = r#"[{"unread":true,"repository":{"full_name":"a/b"}},
                       {"unread":true,"repository":{"full_name":"a/b"}},
                       {"unread":false,"repository":{"full_name":"a/b"}},
                       {"unread":true,"repository":{"full_name":"c/d"}}]"#;
        let n = parse_notifications(json).unwrap();
        assert_eq!(n.total, 3);
        assert_eq!(n.by_repo.get("a/b"), Some(&2));
        assert_eq!(n.by_repo.get("c/d"), Some(&1));
    }

    #[test]
    fn a_missing_unread_flag_counts_as_unread() {
        // /notifications 는 기본이 미확인만 준다
        assert_eq!(parse_notifications(r#"[{},{}]"#).unwrap().total, 2);
        assert_eq!(parse_notifications("[]").unwrap().total, 0);
        assert!(parse_notifications("nope").is_err());
    }

    #[test]
    fn backoff_doubles_but_is_capped() {
        assert_eq!(rate_limit_backoff(Duration::from_secs(300)), Duration::from_secs(600));
        assert_eq!(rate_limit_backoff(Duration::from_secs(600)), Duration::from_secs(900));
        // 상한을 넘지 않는다 — 넘으면 고장난 것처럼 보인다
        assert_eq!(rate_limit_backoff(Duration::from_secs(3600)), Duration::from_secs(900));
    }
}
