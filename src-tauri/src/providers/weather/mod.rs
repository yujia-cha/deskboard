//! 날씨 — [Open-Meteo](https://open-meteo.com). 가입도 API 키도 필요 없다.
//!
//! 위젯이 떠 있을 때만 15분 주기로 받아온다. 위치는 설정의 위도/경도이고,
//! 도시 이름으로 찾는 건 `weather_search` (Open-Meteo Geocoding, 역시 키 불필요).

mod codes;

use super::Provider;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub use codes::{describe, icon_for};

/// 위젯이 떠 있는가. 꺼져 있으면 네트워크를 건드리지 않는다.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// 프론트가 설정한 좌표.
static PLACE: Mutex<Option<(f64, f64)>> = Mutex::new(None);

const POLL: Duration = Duration::from_secs(15 * 60);
/// 실패 후 다시 시도하기까지.
const RETRY: Duration = Duration::from_secs(60);
/// 대기를 이만큼씩 쪼갠다 — 위젯을 막 띄웠거나 도시를 바꿨을 때 15분을 기다리지 않도록.
const SLICE: Duration = Duration::from_secs(2);

/// `total` 만큼 자되, 그 사이에 위젯이 꺼지거나 좌표가 바뀌면 곧바로 깬다.
fn nap(total: Duration, place: Option<(f64, f64)>) {
    let mut left = total;
    while left > Duration::ZERO {
        let step = SLICE.min(left);
        std::thread::sleep(step);
        left = left.saturating_sub(step);
        let now = PLACE.lock().ok().and_then(|p| *p);
        if now != place || !ACTIVE.load(Ordering::Relaxed) {
            return;
        }
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Hour {
    /// "14시" 처럼 보여줄 시각 (로컬)
    pub hour: u8,
    pub temp_c: f64,
    /// 강수 확률 %
    pub pop: u8,
    pub code: u8,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Weather {
    pub temp_c: f64,
    pub feels_c: f64,
    /// 현재 시각 기준 강수 확률 %
    pub pop: u8,
    pub code: u8,
    /// 날씨 코드 설명 ("흐림", "비" …)
    pub text: String,
    pub icon: String,
    pub is_day: bool,
    pub high_c: f64,
    pub low_c: f64,
    pub hourly: Vec<Hour>,
    /// 사람이 읽을 오류. 성공이면 None.
    pub error: Option<String>,
}

/// Open-Meteo 응답 중 우리가 쓰는 부분만.
#[derive(Debug, Deserialize)]
struct ApiResponse {
    current: Current,
    hourly: Hourly,
    daily: Daily,
}
#[derive(Debug, Deserialize)]
struct Current {
    temperature_2m: f64,
    apparent_temperature: f64,
    weather_code: u8,
    is_day: u8,
}
#[derive(Debug, Deserialize)]
struct Hourly {
    time: Vec<String>,
    temperature_2m: Vec<f64>,
    precipitation_probability: Vec<Option<u8>>,
    weather_code: Vec<u8>,
}
#[derive(Debug, Deserialize)]
struct Daily {
    temperature_2m_max: Vec<f64>,
    temperature_2m_min: Vec<f64>,
}

/// 날씨를 어디서 가져오는지. 테스트는 고정 JSON 으로 바꿔 끼운다.
pub trait WeatherSource: Send + Sync {
    fn fetch(&self, lat: f64, lon: f64) -> Result<String, String>;
}

pub struct OpenMeteo;

impl WeatherSource for OpenMeteo {
    fn fetch(&self, lat: f64, lon: f64) -> Result<String, String> {
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}\
             &current=temperature_2m,apparent_temperature,weather_code,is_day\
             &hourly=temperature_2m,precipitation_probability,weather_code\
             &daily=temperature_2m_max,temperature_2m_min\
             &timezone=auto&forecast_days=2"
        );
        let res = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| format!("네트워크 초기화 실패: {e}"))?
            .get(&url)
            .send()
            .map_err(|_| "날씨 서버에 연결하지 못했습니다".to_string())?;
        if !res.status().is_success() {
            return Err(format!("날씨 서버 오류 ({})", res.status().as_u16()));
        }
        res.text().map_err(|e| format!("응답을 읽지 못했습니다: {e}"))
    }
}

/// 응답 JSON 을 위젯이 쓸 모양으로. `now_hour` 는 현재 시각(0~23)으로, 시간별 예보의
/// 시작점을 고르는 데 쓴다 — API 는 오늘 0시부터 전부 주기 때문에 지난 시간을 버려야 한다.
pub fn parse(json: &str, now_hour: u8) -> Result<Weather, String> {
    let r: ApiResponse =
        serde_json::from_str(json).map_err(|e| format!("날씨 응답을 해석하지 못했습니다: {e}"))?;

    let is_day = r.current.is_day == 1;
    let start = r
        .hourly
        .time
        .iter()
        .position(|t| hour_of(t) == Some(now_hour))
        .unwrap_or(0);

    let n = r.hourly.time.len();
    let hourly: Vec<Hour> = (start..n.min(start + 12))
        .filter_map(|i| {
            Some(Hour {
                hour: hour_of(r.hourly.time.get(i)?)?,
                temp_c: *r.hourly.temperature_2m.get(i)?,
                pop: r.hourly.precipitation_probability.get(i).copied().flatten().unwrap_or(0),
                code: *r.hourly.weather_code.get(i)?,
            })
        })
        .collect();

    let code = r.current.weather_code;
    Ok(Weather {
        temp_c: r.current.temperature_2m,
        feels_c: r.current.apparent_temperature,
        pop: hourly.first().map(|h| h.pop).unwrap_or(0),
        code,
        text: describe(code).to_string(),
        icon: icon_for(code, is_day).to_string(),
        is_day,
        high_c: r.daily.temperature_2m_max.first().copied().unwrap_or(f64::NAN),
        low_c: r.daily.temperature_2m_min.first().copied().unwrap_or(f64::NAN),
        hourly,
        error: None,
    })
}

/// "2026-09-21T14:00" → 14
fn hour_of(t: &str) -> Option<u8> {
    t.split('T').nth(1)?.split(':').next()?.parse().ok()
}

/// 프론트가 위젯 표시 여부와 좌표를 알려준다.
#[tauri::command]
pub fn weather_set_active(active: bool, lat: Option<f64>, lon: Option<f64>) {
    ACTIVE.store(active, Ordering::Relaxed);
    if let (Some(lat), Some(lon)) = (lat, lon) {
        if lat.is_finite() && lon.is_finite() && (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
            if let Ok(mut p) = PLACE.lock() {
                *p = Some((lat, lon));
            }
            return;
        }
    }
    if let Ok(mut p) = PLACE.lock() {
        *p = None;
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Place {
    pub name: String,
    pub country: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Deserialize)]
struct GeoResponse {
    results: Option<Vec<GeoHit>>,
}
#[derive(Debug, Deserialize)]
struct GeoHit {
    name: String,
    latitude: f64,
    longitude: f64,
    country: Option<String>,
    admin1: Option<String>,
}

/// 지오코딩 응답 → 후보 목록.
pub fn parse_places(json: &str) -> Result<Vec<Place>, String> {
    let r: GeoResponse =
        serde_json::from_str(json).map_err(|e| format!("검색 결과를 해석하지 못했습니다: {e}"))?;
    Ok(r.results
        .unwrap_or_default()
        .into_iter()
        .map(|h| Place {
            name: match &h.admin1 {
                Some(a) if !a.is_empty() && *a != h.name => format!("{} ({a})", h.name),
                _ => h.name.clone(),
            },
            country: h.country.unwrap_or_default(),
            lat: h.latitude,
            lon: h.longitude,
        })
        .collect())
}

/// 도시 이름으로 좌표를 찾는다. 키 불필요.
#[tauri::command]
pub fn weather_search(query: String) -> Result<Vec<Place>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=5&language=ko&format=json",
        urlencoding(q)
    );
    let body = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("네트워크 초기화 실패: {e}"))?
        .get(&url)
        .send()
        .map_err(|_| "검색 서버에 연결하지 못했습니다".to_string())?
        .text()
        .map_err(|e| format!("응답을 읽지 못했습니다: {e}"))?;
    parse_places(&body)
}

/// 쿼리 문자열에 넣을 수 있게 이스케이프 (도시 이름은 한글·공백이 흔하다).
pub fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub struct WeatherProvider;

impl Provider for WeatherProvider {
    fn id(&self) -> &'static str {
        "weather"
    }

    fn start(&self, app: AppHandle) {
        std::thread::Builder::new()
            .name("weather".into())
            .spawn(move || {
                let src = OpenMeteo;
                loop {
                    let place = PLACE.lock().ok().and_then(|p| *p);
                    // 위젯이 없거나 위치가 없으면 네트워크를 건드리지 않는다.
                    // 짧게 자야 위젯을 띄운 직후 바로 받아온다.
                    if !ACTIVE.load(Ordering::Relaxed) || place.is_none() {
                        std::thread::sleep(SLICE);
                        continue;
                    }
                    let (lat, lon) = place.unwrap();

                    let now_hour = chrono::Local::now().format("%H").to_string().parse().unwrap_or(0);
                    let failed;
                    let snap = match src.fetch(lat, lon).and_then(|b| parse(&b, now_hour)) {
                        Ok(w) => { failed = false; w }
                        Err(e) => {
                            log::warn!("weather: {e}");
                            failed = true;
                            Weather { error: Some(e), ..Default::default() }
                        }
                    };
                    let _ = app.emit("weather://update", snap);

                    // 실패하면 빨리 다시 해본다. 도시를 바꾸면 자다가도 깬다.
                    nap(if failed { RETRY } else { POLL }, place);
                }
            })
            .expect("spawn weather thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "current": { "temperature_2m": 21.4, "apparent_temperature": 19.8, "weather_code": 3, "is_day": 1 },
      "hourly": {
        "time": ["2026-09-21T12:00","2026-09-21T13:00","2026-09-21T14:00","2026-09-21T15:00"],
        "temperature_2m": [20.0, 20.8, 21.4, 21.0],
        "precipitation_probability": [5, 10, 40, null],
        "weather_code": [1, 2, 3, 61]
      },
      "daily": { "temperature_2m_max": [23.5, 24.0], "temperature_2m_min": [14.2, 15.0] }
    }"#;

    #[test]
    fn reads_the_current_conditions() {
        let w = parse(SAMPLE, 14).unwrap();
        assert_eq!(w.temp_c, 21.4);
        assert_eq!(w.feels_c, 19.8);
        assert_eq!(w.code, 3);
        assert!(w.is_day);
        assert_eq!(w.high_c, 23.5);
        assert_eq!(w.low_c, 14.2);
        assert!(!w.text.is_empty());
        assert!(!w.icon.is_empty());
        assert!(w.error.is_none());
    }

    #[test]
    fn hourly_starts_at_the_current_hour_not_at_midnight() {
        let w = parse(SAMPLE, 14).unwrap();
        assert_eq!(w.hourly.first().map(|h| h.hour), Some(14));
        // 지난 시간(12, 13시)은 버린다
        assert!(w.hourly.iter().all(|h| h.hour >= 14));
    }

    #[test]
    fn the_headline_chance_of_rain_comes_from_the_current_hour() {
        assert_eq!(parse(SAMPLE, 14).unwrap().pop, 40);
        assert_eq!(parse(SAMPLE, 12).unwrap().pop, 5);
    }

    #[test]
    fn a_missing_probability_reads_as_zero_rather_than_failing() {
        let w = parse(SAMPLE, 15).unwrap();
        assert_eq!(w.hourly.first().map(|h| h.pop), Some(0));
    }

    #[test]
    fn an_unknown_hour_falls_back_to_the_start_of_the_data() {
        let w = parse(SAMPLE, 23).unwrap();
        assert_eq!(w.hourly.first().map(|h| h.hour), Some(12));
    }

    #[test]
    fn broken_json_reports_a_readable_error() {
        let e = parse("not json", 12).unwrap_err();
        assert!(e.contains("해석하지 못했습니다"), "{e}");
    }

    #[test]
    fn is_day_reaches_the_icon() {
        // 맑음(0)은 밤낮 아이콘이 다르다. 흐림(3)은 같은 게 맞으므로 샘플의 코드를 바꿔서 본다.
        let clear_day = SAMPLE.replace("\"weather_code\": 3", "\"weather_code\": 0");
        let clear_night = clear_day.replace("\"is_day\": 1", "\"is_day\": 0");
        let d = parse(&clear_day, 14).unwrap();
        let n = parse(&clear_night, 14).unwrap();
        assert!(d.is_day && !n.is_day);
        assert_eq!(d.code, n.code);
        assert_ne!(d.icon, n.icon, "맑음은 밤낮 아이콘이 달라야 한다");
    }

    #[test]
    fn overcast_looks_the_same_at_night() {
        let night = SAMPLE.replace("\"is_day\": 1", "\"is_day\": 0");
        assert_eq!(parse(SAMPLE, 14).unwrap().icon, parse(&night, 14).unwrap().icon);
    }

    #[test]
    fn geocoding_results_are_flattened_with_their_region() {
        let json = r#"{"results":[
          {"name":"Seoul","latitude":37.57,"longitude":126.98,"country":"대한민국","admin1":"서울"},
          {"name":"Busan","latitude":35.1,"longitude":129.04,"country":"대한민국","admin1":"Busan"}
        ]}"#;
        let p = parse_places(json).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].name, "Seoul (서울)");
        assert_eq!(p[0].lat, 37.57);
        // 지역명이 도시명과 같으면 중복해서 붙이지 않는다
        assert_eq!(p[1].name, "Busan");
    }

    #[test]
    fn no_geocoding_results_is_an_empty_list_not_an_error() {
        assert_eq!(parse_places(r#"{}"#).unwrap().len(), 0);
        assert_eq!(parse_places(r#"{"results":[]}"#).unwrap().len(), 0);
    }

    #[test]
    fn city_names_with_spaces_and_hangul_survive_the_url() {
        assert_eq!(urlencoding("New York"), "New%20York");
        assert_eq!(urlencoding("서울"), "%EC%84%9C%EC%9A%B8");
        assert_eq!(urlencoding("abc-1_2.3~"), "abc-1_2.3~");
    }
}
