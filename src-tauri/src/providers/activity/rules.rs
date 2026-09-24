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

/// 게임 설치 폴더 안에서 실행됐는가. 처음 보는 프로그램을 자동으로 게임 규칙에 넣을지
/// 판단하는 데만 쓴다 — 경로 자체는 저장하지 않는다.
pub fn looks_like_game_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase().replace('/', "\\");
    const MARKERS: [&str; 9] = [
        "\\steamapps\\common\\",
        "\\epic games\\",
        "\\gog galaxy\\games\\",
        "\\riot games\\",
        "\\xboxgames\\",
        "\\ubisoft game launcher\\games\\",
        "\\ea games\\",
        "\\battle.net\\",
        "\\nexon\\",
    ];
    MARKERS.iter().any(|m| p.contains(m))
}

/// 게임 설치 폴더 안에 있어도 자동 분류에서 제외할 실행 파일 (정규화된 이름).
/// Wallpaper Engine 은 `steamapps\common` 안에 있지만 배경화면 프로그램이지 게임이 아니다.
const AUTO_GAME_EXCLUDE: [&str; 4] = ["wallpaper32", "wallpaper64", "ui32", "ui64"];

/// 경로로 게임을 자동 태깅할 때, 이 실행 파일은 예외로 둘지.
pub fn is_auto_game_excluded(normalized_exe: &str) -> bool {
    AUTO_GAME_EXCLUDE.contains(&normalized_exe)
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

    #[test]
    fn recognizes_known_game_install_folders() {
        assert!(looks_like_game_path(
            r"C:\Program Files (x86)\Steam\steamapps\common\Elden Ring\eldenring.exe"
        ));
        assert!(looks_like_game_path(
            r"C:\Program Files\Epic Games\Fortnite\FortniteClient.exe"
        ));
        assert!(looks_like_game_path(
            r"C:\Riot Games\League of Legends\LeagueClient.exe"
        ));
    }

    #[test]
    fn does_not_flag_ordinary_programs() {
        assert!(!looks_like_game_path(r"C:\Program Files\Mozilla Firefox\firefox.exe"));
    }

    #[test]
    fn handles_forward_slashes_too() {
        assert!(looks_like_game_path(
            "C:/Program Files (x86)/Steam/steamapps/common/Elden Ring/eldenring.exe"
        ));
    }

    #[test]
    fn wallpaper_engine_is_excluded_from_auto_game() {
        assert!(is_auto_game_excluded("wallpaper64"));
        assert!(is_auto_game_excluded("wallpaper32"));
        assert!(is_auto_game_excluded("ui64"));
        assert!(!is_auto_game_excluded("eldenring"));
    }
}
