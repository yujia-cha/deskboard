//! 자동 갱신 시점 판단 — 순수 함수.

use chrono::{DateTime, TimeZone};

/// 실패한 뒤 자동으로 다시 시도하기까지 기다리는 시간.
const RETRY_AFTER_FAILURE_MS: u64 = 30 * 60 * 1000;

pub struct Schedule<'a> {
    pub refresh: &'a str,
    pub daily_hour: u32,
    pub prompt_empty: bool,
    /// 저장된 결과가 지금 주제와 다른 주제의 것이다
    pub prompt_changed: bool,
}

/// 지금 자동으로 돌려야 하는가. 주제가 비었거나 바뀌었으면 절대 자동으로 돌리지 않는다.
pub fn due<Tz: TimeZone>(s: &Schedule, fetched_at: Option<u64>, attempted_at: Option<u64>, now: DateTime<Tz>) -> bool {
    if s.prompt_empty || s.prompt_changed {
        return false;
    }
    let now_ms = now.timestamp_millis().max(0) as u64;
    if let (Some(a), f) = (attempted_at, fetched_at.unwrap_or(0)) {
        if a > f && now_ms < a + RETRY_AFTER_FAILURE_MS {
            return false;
        }
    }
    let period_ms = match s.refresh {
        "3h" => 3 * 3_600_000u64,
        "6h" => 6 * 3_600_000u64,
        "daily" => {
            let Some(boundary) = now.timezone().from_local_datetime(&now.date_naive().and_hms_opt(s.daily_hour.min(23), 0, 0).unwrap()).earliest() else {
                return false;
            };
            if now < boundary {
                return false;
            }
            let b = boundary.timestamp_millis().max(0) as u64;
            return fetched_at.is_none_or(|f| f < b);
        }
        _ => return false,
    };
    fetched_at.is_none_or(|f| now_ms >= f + period_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, FixedOffset, TimeZone};

    fn at(h: u32, m: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(9 * 3600).unwrap().with_ymd_and_hms(2026, 5, 10, h, m, 0).unwrap()
    }
    fn ms(d: DateTime<FixedOffset>) -> u64 {
        d.timestamp_millis() as u64
    }
    fn sch(refresh: &str) -> Schedule<'_> {
        Schedule { refresh, daily_hour: 8, prompt_empty: false, prompt_changed: false }
    }

    #[test]
    fn off_empty_and_changed_never_run() {
        assert!(!due(&sch("off"), None, None, at(12, 0)));
        assert!(!due(&Schedule { prompt_empty: true, ..sch("3h") }, None, None, at(12, 0)));
        assert!(!due(&Schedule { prompt_changed: true, ..sch("3h") }, None, None, at(12, 0)));
    }

    #[test]
    fn hourly_periods() {
        let f = Some(ms(at(9, 0)));
        assert!(due(&sch("3h"), None, None, at(9, 0)));
        assert!(!due(&sch("3h"), f, None, at(11, 59)));
        assert!(due(&sch("3h"), f, None, at(12, 0)));
        assert!(!due(&sch("6h"), f, None, at(14, 59)));
        assert!(due(&sch("6h"), f, None, at(15, 0)));
    }

    #[test]
    fn daily_boundary() {
        // 08:00 기준
        assert!(!due(&sch("daily"), Some(ms(at(1, 0))), None, at(7, 59)));
        assert!(due(&sch("daily"), Some(ms(at(1, 0))), None, at(8, 0)));
        assert!(!due(&sch("daily"), Some(ms(at(8, 5))), None, at(23, 0)));
        assert!(due(&sch("daily"), None, None, at(9, 0)));
        // 어제 오후에 받았고 오늘 8시를 지났다
        let yesterday = at(15, 0) - Duration::days(1);
        assert!(due(&sch("daily"), Some(ms(yesterday)), None, at(8, 1)));
    }

    #[test]
    fn failure_backs_off() {
        let f = Some(ms(at(0, 0)));
        let a = Some(ms(at(12, 0)));
        assert!(!due(&sch("3h"), f, a, at(12, 29)));
        assert!(due(&sch("3h"), f, a, at(12, 30)));
        assert!(!due(&sch("3h"), None, a, at(12, 10)));
    }
}
