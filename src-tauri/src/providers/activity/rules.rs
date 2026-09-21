//! 실행 파일 → 분류(게임/작업/기타).
//!
//! 기본 규칙은 `resources/activity-rules.json` 에서 읽고, 사용자가 위젯에서 바꾼 것이 우선한다.
//! 전부 순수 함수라 그대로 테스트한다.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Game,
    Work,
    Other,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Game => "game",
            Category::Work => "work",
            Category::Other => "other",
        }
    }

    /// 저장된 문자열에서. 모르는 값은 기타로 본다.
    pub fn parse(s: &str) -> Category {
        match s.trim().to_ascii_lowercase().as_str() {
            "game" => Category::Game,
            "work" => Category::Work,
            _ => Category::Other,
        }
    }
}

/// 비교용으로 정규화한 실행 파일 이름 — 소문자, `.exe` 제거, 경로 제거.
pub fn normalize_exe(exe: &str) -> String {
    let base = exe
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(exe)
        .trim()
        .to_ascii_lowercase();
    base.strip_suffix(".exe").unwrap_or(&base).to_string()
}

/// 기본 규칙 묶음. 키는 정규화된 이름.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Seed {
    #[serde(default)]
    pub game: Vec<String>,
    #[serde(default)]
    pub work: Vec<String>,
}

impl Seed {
    pub fn to_map(&self) -> HashMap<String, Category> {
        let mut m = HashMap::new();
        for e in &self.game {
            m.insert(normalize_exe(e), Category::Game);
        }
        for e in &self.work {
            m.insert(normalize_exe(e), Category::Work);
        }
        m
    }

    pub fn from_json(s: &str) -> Result<Seed, String> {
        serde_json::from_str(s).map_err(|e| format!("분류 규칙을 읽지 못했습니다: {e}"))
    }
}

/// 실행 파일의 분류. 사용자가 정한 것이 기본 규칙을 덮어쓴다.
pub fn categorize(
    exe: &str,
    user: &HashMap<String, Category>,
    seed: &HashMap<String, Category>,
) -> Category {
    let key = normalize_exe(exe);
    user.get(&key)
        .or_else(|| seed.get(&key))
        .copied()
        .unwrap_or(Category::Other)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, Category)]) -> HashMap<String, Category> {
        pairs.iter().map(|(k, v)| (normalize_exe(k), *v)).collect()
    }

    #[test]
    fn normalizes_paths_case_and_extension() {
        assert_eq!(normalize_exe("C:\\Games\\EldenRing.exe"), "eldenring");
        assert_eq!(normalize_exe("Code.EXE"), "code");
        assert_eq!(normalize_exe("  chrome.exe  "), "chrome");
        assert_eq!(normalize_exe("/usr/bin/vim"), "vim");
        assert_eq!(normalize_exe("plain"), "plain");
    }

    #[test]
    fn a_name_that_is_only_an_extension_survives() {
        // ".exe" 를 통째로 지워서 빈 문자열이 되면 규칙이 전부 매칭돼 버린다
        assert_eq!(normalize_exe(".exe"), "");
        assert_eq!(normalize_exe(""), "");
    }

    #[test]
    fn unknown_programs_are_other() {
        assert_eq!(categorize("mystery.exe", &map(&[]), &map(&[])), Category::Other);
    }

    #[test]
    fn seed_rules_classify_known_programs() {
        let seed = map(&[("eldenring.exe", Category::Game), ("Code.exe", Category::Work)]);
        assert_eq!(categorize("EldenRing.exe", &map(&[]), &seed), Category::Game);
        assert_eq!(categorize("code.exe", &map(&[]), &seed), Category::Work);
    }

    #[test]
    fn user_rules_win_over_the_seed() {
        let seed = map(&[("chrome.exe", Category::Other)]);
        let user = map(&[("chrome.exe", Category::Work)]);
        assert_eq!(categorize("chrome.exe", &user, &seed), Category::Work);
    }

    #[test]
    fn matching_ignores_path_and_case() {
        let seed = map(&[("steam.exe", Category::Game)]);
        assert_eq!(categorize("D:\\Steam\\STEAM.EXE", &map(&[]), &seed), Category::Game);
    }

    #[test]
    fn category_round_trips_through_its_stored_string() {
        for c in [Category::Game, Category::Work, Category::Other] {
            assert_eq!(Category::parse(c.as_str()), c);
        }
    }

    #[test]
    fn unknown_stored_categories_degrade_to_other() {
        assert_eq!(Category::parse("nonsense"), Category::Other);
        assert_eq!(Category::parse(""), Category::Other);
        assert_eq!(Category::parse("GAME"), Category::Game);
    }

    #[test]
    fn seed_json_loads_both_lists() {
        let s = Seed::from_json(r#"{"game":["EldenRing.exe"],"work":["Code.exe","WindowsTerminal.exe"]}"#).unwrap();
        let m = s.to_map();
        assert_eq!(m.get("eldenring"), Some(&Category::Game));
        assert_eq!(m.get("code"), Some(&Category::Work));
        assert_eq!(m.get("windowsterminal"), Some(&Category::Work));
    }

    #[test]
    fn broken_seed_json_reports_an_error() {
        assert!(Seed::from_json("not json").is_err());
    }

    #[test]
    fn a_missing_list_is_not_an_error() {
        let s = Seed::from_json(r#"{"game":["a.exe"]}"#).unwrap();
        assert_eq!(s.to_map().len(), 1);
        assert_eq!(Seed::from_json("{}").unwrap().to_map().len(), 0);
    }
}
