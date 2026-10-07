//! RSS 백엔드 — Google 뉴스 검색 RSS. 무료이고 키가 필요 없지만 요약은 없다.

use super::api::{clean_items, RunOutput, Settings};
use super::ScrapItem;
use serde_json::{json, Value};
use std::time::Duration;

const FEED: &str = "https://news.google.com/rss/search";

/// 검색어에서 뺄 군더더기 — 주제는 Claude 에게 쓰듯 문장으로 적기 쉬운데, Google 뉴스는 단어를 **모두** 포함한
/// 기사만 돌려준다. "Rust 프로그래밍 언어 생태계 새 소식" 은 실측 0건이었다.
const FILLER: &[&str] = &[
    "새", "새로운", "소식", "뉴스", "최신", "최근", "관련", "동향", "소식들", "이번", "주", "정보", "기사", "자료", "글", "것",
    "the", "a", "an", "news", "latest", "new", "recent", "about", "of", "and", "on", "in", "for", "updates",
];

pub fn keywords(prompt: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in prompt.split(|c: char| c.is_whitespace() || ",·/|\"'“”‘’()[]".contains(c)) {
        let w = w.trim();
        if w.is_empty() || FILLER.contains(&w.to_lowercase().as_str()) || out.iter().any(|o| o.eq_ignore_ascii_case(w)) {
            continue;
        }
        out.push(w.to_string());
    }
    out
}

/// 차례로 넓혀 갈 검색 주제: 원문 → 군더더기 뺀 단어들 → 앞의 두 단어 → 첫 단어.
/// 앞의 것일수록 정확하고, 결과가 0건일 때만 다음으로 넘어간다.
pub fn topics(prompt: &str) -> Vec<String> {
    let kw = keywords(prompt);
    let mut out: Vec<String> = Vec::new();
    for t in [prompt.trim().to_string(), kw.join(" "), kw.iter().take(2).cloned().collect::<Vec<_>>().join(" "), kw.first().cloned().unwrap_or_default()] {
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// Google 뉴스 검색어 — 주제 + 기간 연산자 + 사이트 연산자.
#[cfg(test)]
pub fn build_query(s: &Settings) -> String {
    build_query_for(s, &s.prompt)
}

fn build_query_for(s: &Settings, topic: &str) -> String {
    let mut q = topic.to_string();
    match s.recency.as_str() {
        "day" => q.push_str(" when:1d"),
        "week" => q.push_str(" when:7d"),
        "month" => q.push_str(" when:30d"),
        _ => {}
    }
    match s.domain_mode.as_str() {
        "allow" if !s.domains.is_empty() => {
            let sites: Vec<String> = s.domains.iter().map(|d| format!("site:{d}")).collect();
            q.push_str(&format!(" ({})", sites.join(" OR ")));
        }
        "block" if !s.blocked_domains.is_empty() => {
            for d in &s.blocked_domains {
                q.push_str(&format!(" -site:{d}"));
            }
        }
        _ => {}
    }
    q
}

#[cfg(test)]
pub fn build_url(s: &Settings) -> String {
    build_url_for(s, &s.prompt)
}

fn build_url_for(s: &Settings, topic: &str) -> String {
    let (hl, gl, ceid) = if s.language == "en" { ("en-US", "US", "US:en") } else { ("ko", "KR", "KR:ko") };
    url::Url::parse_with_params(FEED, [("q", build_query_for(s, topic).as_str()), ("hl", hl), ("gl", gl), ("ceid", ceid)])
        .map(String::from)
        .unwrap_or_else(|_| FEED.to_string())
}

/// 태그를 떼고 흔한 엔티티를 풀어 한 줄로 — description 은 같은 제목으로 가는 링크 HTML 이다.
fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.replace("&nbsp;", " ").replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn local_date(pub_date: &str) -> String {
    chrono::DateTime::parse_from_rfc2822(pub_date.trim()).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string()).unwrap_or_default()
}

fn child_text<'a>(item: roxmltree::Node<'a, '_>, tag: &str) -> &'a str {
    item.children().find(|n| n.has_tag_name(tag)).and_then(|n| n.text()).unwrap_or("").trim()
}

pub fn parse_feed(xml: &str, count: usize) -> Result<Vec<ScrapItem>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("RSS 를 해석하지 못했습니다: {e}"))?;
    let mut items: Vec<Value> = Vec::new();
    for item in doc.descendants().filter(|n| n.has_tag_name("item")) {
        let raw_title = child_text(item, "title");
        let source = child_text(item, "source");
        // Google 은 제목 뒤에 " - 매체" 를 붙인다
        let title = match raw_title.strip_suffix(source).and_then(|t| t.strip_suffix(" - ")) {
            Some(t) if !source.is_empty() => t.trim(),
            _ => raw_title,
        };
        let desc = strip_html(child_text(item, "description"));
        let same_as_title = desc.is_empty()
            || desc == title
            || desc == raw_title
            || [title, raw_title].iter().any(|t| desc.strip_prefix(t).is_some_and(|rest| rest.trim() == source || rest.trim().is_empty()));
        items.push(json!({
            "title": title,
            "url": child_text(item, "link"),
            "source": source,
            "published": local_date(child_text(item, "pubDate")),
            "summary": if same_as_title { String::new() } else { desc },
        }));
    }
    Ok(clean_items(&json!({ "items": items }), count))
}

pub async fn run(s: &Settings) -> Result<RunOutput, String> {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(20)).user_agent("deskboard").build().map_err(|e| e.to_string())?;
    let fetch = |topic: String| {
        let http = http.clone();
        let url = build_url_for(s, &topic);
        async move {
            let res = http.get(url).send().await.map_err(|e| format!("네트워크 오류: {}", e.without_url()))?;
            if !res.status().is_success() {
                return Err(format!("RSS 오류 ({})", res.status().as_u16()));
            }
            res.text().await.map_err(|e| format!("응답 읽기 실패: {}", e.without_url()))
        }
    };
    run_with(s, fetch).await
}

/// 주제를 차례로 넓혀 가며 처음으로 결과가 나온 검색을 쓴다 (`topics`). 네트워크 부분은 주입받는다.
async fn run_with<F, Fut>(s: &Settings, mut fetch: F) -> Result<RunOutput, String>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let candidates = topics(&s.prompt);
    for (i, topic) in candidates.iter().enumerate() {
        let items = parse_feed(&fetch(topic.clone()).await?, s.count)?;
        if !items.is_empty() {
            let note = (i > 0).then(|| format!("RSS 는 단어 검색이라 \"{topic}\" 로 넓혀서 찾았습니다"));
            return Ok(RunOutput { items, usage: None, note });
        }
    }
    Err("조건에 맞는 항목을 찾지 못했습니다 — RSS 는 단어 검색이라 짧은 키워드(예: \"Rust\")가 잘 찾습니다".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(v: Value) -> Settings {
        Settings::from_value(&v)
    }

    const FIXTURE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<rss xmlns:media="http://search.yahoo.com/mrss/" version="2.0"><channel>
<generator>NFE/5.0</generator><title>"Rust 프로그래밍" - Google 뉴스</title><link>https://news.google.com/search?q=x</link><language>ko</language>
<item><title>Rust 1.90 출시, 컴파일 속도 개선 - 지디넷코리아</title><link>https://news.google.com/rss/articles/CBMiA1?oc=5</link><guid isPermaLink="false">CBMiA1</guid><pubDate>Thu, 01 Oct 2026 07:00:00 GMT</pubDate><description>&lt;a href="https://news.google.com/rss/articles/CBMiA1?oc=5" target="_blank"&gt;Rust 1.90 출시, 컴파일 속도 개선&lt;/a&gt;&amp;nbsp;&amp;nbsp;&lt;font color="#6f6f6f"&gt;지디넷코리아&lt;/font&gt;</description><source url="https://zdnet.co.kr">지디넷코리아</source></item>
<item><title>같은 기사 - 다른 제목</title><link>https://news.google.com/rss/articles/CBMiA1?oc=5</link><pubDate>Thu, 01 Oct 2026 08:00:00 GMT</pubDate><description>&lt;a href="x"&gt;다른 요약 문장&lt;/a&gt;</description><source url="https://b.com">B</source></item>
<item><title>제목 - 매체 - 연재</title><link>https://news.google.com/rss/articles/CBMiA2?oc=5</link><pubDate>garbage</pubDate><description>&lt;a href="y"&gt;제목 - 매체 - 연재&lt;/a&gt;&amp;nbsp;&amp;nbsp;&lt;font&gt;연재&lt;/font&gt;</description><source url="https://c.com">연재</source></item>
<item><title>위험한 링크</title><link>javascript:alert(1)</link><source url="https://d.com">D</source></item>
<item><title>세 번째 유효</title><link>https://news.google.com/rss/articles/CBMiA3?oc=5</link><source url="https://e.com">E</source></item>
</channel></rss>"##;

    #[test]
    fn query_recency_and_domains() {
        assert_eq!(build_query(&settings(json!({ "prompt": "rust", "recency": "day" }))), "rust when:1d");
        assert_eq!(build_query(&settings(json!({ "prompt": "rust", "recency": "week" }))), "rust when:7d");
        assert_eq!(build_query(&settings(json!({ "prompt": "rust", "recency": "month" }))), "rust when:30d");
        assert_eq!(build_query(&settings(json!({ "prompt": "rust", "recency": "any" }))), "rust");
        let allow = settings(json!({ "prompt": "rust", "recency": "any", "domainMode": "allow", "domains": "a.com b.org", "blockedDomains": "c.com" }));
        assert_eq!(build_query(&allow), "rust (site:a.com OR site:b.org)");
        let block = settings(json!({ "prompt": "rust", "recency": "day", "domainMode": "block", "domains": "a.com", "blockedDomains": "c.com, d.org" }));
        assert_eq!(build_query(&block), "rust when:1d -site:c.com -site:d.org");
        assert_eq!(build_query(&settings(json!({ "prompt": "rust", "recency": "any", "domainMode": "allow" }))), "rust");
    }

    #[test]
    fn url_encodes_and_picks_locale() {
        let ko = build_url(&settings(json!({ "prompt": "Rust 프로그래밍", "recency": "week" })));
        assert!(ko.starts_with("https://news.google.com/rss/search?q=Rust+%ED%94%84"), "{ko}");
        assert!(ko.contains("when%3A7d") && ko.ends_with("&hl=ko&gl=KR&ceid=KR%3Ako"), "{ko}");
        let en = build_url(&settings(json!({ "prompt": "rust", "language": "en" })));
        assert!(en.ends_with("&hl=en-US&gl=US&ceid=US%3Aen"), "{en}");
    }

    #[test]
    fn parses_fixture() {
        let items = parse_feed(FIXTURE, 8).unwrap();
        // 링크가 같은 둘째는 중복, javascript: 는 제외
        assert_eq!(items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["Rust 1.90 출시, 컴파일 속도 개선", "제목 - 매체", "세 번째 유효"]);
        assert_eq!(items[0].source, "지디넷코리아");
        assert_eq!(items[0].summary, "");
        // 날짜는 로컬 시간대로 — 어느 시간대든 10/01 이거나 10/02 이다
        assert!(["2026-10-01", "2026-10-02"].contains(&items[0].published.as_str()), "{}", items[0].published);
        assert_eq!(items[1].published, "");
        assert_eq!(items[1].summary, "");
    }

    #[test]
    fn keeps_distinct_description_and_caps_count() {
        let items = parse_feed(FIXTURE.replace("같은 기사 - 다른 제목", "다른 기사 - B").replace("CBMiA1?oc=5</link><pubDate>Thu, 01 Oct 2026 08", "CBMiA9?oc=5</link><pubDate>Thu, 01 Oct 2026 08").as_str(), 2).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].title, "다른 기사");
        assert_eq!(items[1].summary, "다른 요약 문장");
    }

    #[test]
    fn bad_xml_is_error() {
        assert!(parse_feed("<rss><channel>", 8).is_err());
        assert!(parse_feed("<rss><channel></channel></rss>", 8).unwrap().is_empty());
    }

    #[test]
    fn keywords_drop_filler_and_topics_widen() {
        assert_eq!(keywords("Rust 프로그래밍 언어 생태계 새 소식"), ["Rust", "프로그래밍", "언어", "생태계"]);
        assert_eq!(keywords("latest news about Rust, rust"), ["Rust"]);
        assert_eq!(
            topics("Rust 프로그래밍 언어 생태계 새 소식"),
            ["Rust 프로그래밍 언어 생태계 새 소식", "Rust 프로그래밍 언어 생태계", "Rust 프로그래밍", "Rust"]
        );
        assert_eq!(topics("Rust"), ["Rust"]);
        assert!(topics("   ").is_empty());
    }

    #[tokio::test]
    async fn widens_until_something_is_found() {
        let s = settings(json!({ "prompt": "Rust 프로그래밍 새 소식", "recency": "week" }));
        let seen = std::sync::Mutex::new(Vec::new());
        let empty = "<rss><channel></channel></rss>".to_string();
        let out = run_with(&s, |t| {
            seen.lock().unwrap().push(t.clone());
            let body = if t == "Rust" { FIXTURE.to_string() } else { empty.clone() };
            async move { Ok(body) }
        })
        .await
        .unwrap();
        assert_eq!(*seen.lock().unwrap(), ["Rust 프로그래밍 새 소식", "Rust 프로그래밍", "Rust"]);
        assert!(out.note.unwrap().contains("\"Rust\""));
        // 처음에 찾으면 안내가 없다
        let out = run_with(&s, |_| async { Ok(FIXTURE.to_string()) }).await.unwrap();
        assert!(out.note.is_none());
        // 끝까지 없으면 키워드 안내와 함께 실패
        let err = run_with(&s, |_| async { Ok("<rss><channel></channel></rss>".to_string()) }).await.unwrap_err();
        assert!(err.contains("키워드"));
    }

    #[tokio::test]
    #[ignore]
    async fn rss_live() {
        let out = run(&settings(json!({ "prompt": "Rust 프로그래밍", "recency": "month" }))).await.unwrap();
        println!("items = {}", out.items.len());
        for i in &out.items {
            println!("- {} | {} | {} | {}", i.title, i.source, i.published, i.url);
        }
        assert!(!out.items.is_empty());
    }
}
