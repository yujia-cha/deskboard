//! WMO 날씨 코드 → 한국어 설명과 아이콘.
//!
//! Open-Meteo 는 WMO 4677 코드를 준다. 값이 듬성듬성해서(0,1,2,3,45,48,51,…) 배열이 아니라
//! 구간으로 매칭한다. 모르는 코드는 조용히 "알 수 없음" 으로 떨어뜨린다.

/// 사람이 읽을 설명.
pub fn describe(code: u8) -> &'static str {
    match code {
        0 => "맑음",
        1 => "대체로 맑음",
        2 => "구름 조금",
        3 => "흐림",
        45 | 48 => "안개",
        51 | 53 | 55 => "이슬비",
        56 | 57 => "어는 이슬비",
        61 => "약한 비",
        63 => "비",
        65 => "강한 비",
        66 | 67 => "어는 비",
        71 => "약한 눈",
        73 => "눈",
        75 => "강한 눈",
        77 => "싸락눈",
        80 | 81 => "소나기",
        82 => "강한 소나기",
        85 | 86 => "소낙눈",
        95 => "천둥번개",
        96 | 99 => "우박을 동반한 천둥번개",
        _ => "알 수 없음",
    }
}

/// 이모지 아이콘. 밤에는 맑음·구름 계열만 달라진다 (비는 밤낮이 같다).
pub fn icon_for(code: u8, is_day: bool) -> &'static str {
    match code {
        0 => if is_day { "☀️" } else { "🌙" },
        1 => if is_day { "🌤️" } else { "🌙" },
        2 => if is_day { "⛅" } else { "☁️" },
        3 => "☁️",
        45 | 48 => "🌫️",
        51 | 53 | 55 | 56 | 57 => "🌦️",
        61 | 63 | 65 | 66 | 67 => "🌧️",
        71 | 73 | 75 | 77 | 85 | 86 => "❄️",
        80 | 81 | 82 => "🌦️",
        95 | 96 | 99 => "⛈️",
        _ => "❔",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_all_describe_something_specific() {
        for c in [0u8, 1, 2, 3, 45, 48, 51, 53, 55, 61, 63, 65, 71, 73, 75, 80, 82, 95, 96, 99] {
            assert_ne!(describe(c), "알 수 없음", "code {c}");
            assert_ne!(icon_for(c, true), "❔", "code {c}");
        }
    }

    #[test]
    fn unknown_codes_degrade_quietly() {
        for c in [4u8, 7, 30, 100, 200, 255] {
            assert_eq!(describe(c), "알 수 없음");
            assert_eq!(icon_for(c, true), "❔");
        }
    }

    #[test]
    fn clear_and_cloudy_change_at_night_but_rain_does_not() {
        assert_ne!(icon_for(0, true), icon_for(0, false));
        assert_ne!(icon_for(1, true), icon_for(1, false));
        assert_ne!(icon_for(2, true), icon_for(2, false));
        // 비·눈·천둥은 밤낮 구분이 의미 없다
        for c in [63u8, 73, 95] {
            assert_eq!(icon_for(c, true), icon_for(c, false), "code {c}");
        }
    }

    #[test]
    fn rain_intensity_is_distinguishable_in_words() {
        assert_ne!(describe(61), describe(65));
        assert_ne!(describe(71), describe(75));
    }

    #[test]
    fn never_panics_over_the_whole_range() {
        for c in 0u8..=255 {
            let _ = describe(c);
            let _ = icon_for(c, c % 2 == 0);
        }
    }
}
