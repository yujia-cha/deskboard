//! 인스턴스별 결과 캐시 — `scrap/<instanceId>.json`. 재시작해도 API 를 다시 부르지 않게 한다.

use super::api::Usage;
use super::ScrapItem;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stored {
    pub items: Vec<ScrapItem>,
    /// 마지막으로 **성공한** 시각 (unix ms)
    pub fetched_at: Option<u64>,
    /// 마지막으로 시도한 시각 — 실패 뒤 곧바로 되풀이하지 않게 한다
    pub attempted_at: Option<u64>,
    pub prompt_hash: String,
    pub usage: Option<Usage>,
    pub auth: Option<String>,
    pub model: Option<String>,
    pub error: Option<String>,
    /// 대체 백엔드로 가져왔다는 안내 — 다음 성공(대체 없음)에서 지운다
    pub note: Option<String>,
    /// 마지막 시도 때 **쓸 수 있었던** 백엔드들 (`ScrapState.auth` 값, 시도 순서).
    /// 그 뒤 더 나은 것이 생기면(로그인·키 입력) 주기를 기다리지 않고 한 번 다시 찾는다.
    pub tried: Vec<String>,
}

pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn path(dir: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("잘못된 인스턴스 id".into());
    }
    Ok(dir.join("scrap").join(format!("{id}.json")))
}

pub fn load(dir: &Path, id: &str) -> Stored {
    path(dir, id)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, id: &str, s: &Stored) -> Result<(), String> {
    let p = path(dir, id)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_validation() {
        assert!(valid_id("abc-123"));
        assert!(!valid_id(""));
        assert!(!valid_id("../x"));
        assert!(!valid_id("a_b"));
        assert!(!valid_id(&"a".repeat(65)));
        assert!(valid_id(&"a".repeat(64)));
    }

    #[test]
    fn roundtrip_and_bad_id() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path(), "w1"), Stored::default());
        let s = Stored {
            items: vec![ScrapItem { title: "t".into(), url: "https://a.com".into(), source: "s".into(), published: "".into(), summary: "x".into() }],
            fetched_at: Some(5),
            prompt_hash: "h".into(),
            usage: Some(Usage { input_tokens: 1, output_tokens: 2, web_searches: 3 }),
            ..Default::default()
        };
        save(dir.path(), "w1", &s).unwrap();
        assert_eq!(load(dir.path(), "w1"), s);
        assert!(save(dir.path(), "../evil", &s).is_err());
        assert_eq!(load(dir.path(), "../evil"), Stored::default());
    }
}
