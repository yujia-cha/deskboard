//! LibreHardwareMonitor 웹서버(`http://localhost:8085/data.json`)에서 CPU 패키지 온도를 읽는다.
//! Windows 사용자 모드에서는 CPU 온도를 직접 읽을 수 없어 LHM 이 켜져 있을 때만 동작한다.
//! 연결 실패 시 30초마다 재시도.

use super::{SensorSample, SensorSource};
use serde_json::Value;
use std::time::{Duration, Instant};

const URL: &str = "http://localhost:8085/data.json";

pub struct LhmSource {
    client: reqwest::blocking::Client,
    disabled_until: Option<Instant>,
}

impl LhmSource {
    pub fn new() -> Self {
        Self {
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_millis(800))
                .build()
                .expect("reqwest client"),
            disabled_until: None,
        }
    }
}

/// LHM 트리에서 CPU 온도 노드를 찾는다. 우선순위: "CPU Package" > "Core (Tctl/Tdie)" > 첫 "CPU Core".
pub fn find_cpu_temp(root: &Value) -> Option<f32> {
    let mut found: Vec<(u8, f32)> = Vec::new();
    walk(root, false, &mut found);
    found.sort_by_key(|(p, _)| *p);
    found.first().map(|(_, v)| *v)
}

fn walk(node: &Value, in_temps: bool, found: &mut Vec<(u8, f32)>) {
    let text = node.get("Text").and_then(Value::as_str).unwrap_or("");
    let in_temps = in_temps || text == "Temperatures";
    if in_temps {
        if let Some(v) = node.get("Value").and_then(Value::as_str) {
            if v.contains("°C") {
                let prio = if text == "CPU Package" {
                    0
                } else if text.starts_with("Core (Tctl") || text.starts_with("CPU (Tctl") {
                    1
                } else if text.starts_with("CPU Core") || text.starts_with("Core #") {
                    2
                } else {
                    255
                };
                if prio != 255 {
                    if let Some(n) = parse_celsius(v) {
                        found.push((prio, n));
                    }
                }
            }
        }
    }
    if let Some(children) = node.get("Children").and_then(Value::as_array) {
        for c in children {
            walk(c, in_temps, found);
        }
    }
}

fn parse_celsius(s: &str) -> Option<f32> {
    s.trim_end_matches("°C").trim().replace(',', ".").parse().ok()
}

impl SensorSource for LhmSource {
    fn name(&self) -> &'static str {
        "librehardwaremonitor"
    }

    fn sample(&mut self, out: &mut SensorSample) {
        if let Some(until) = self.disabled_until {
            if Instant::now() < until {
                return;
            }
            self.disabled_until = None;
        }
        let result = self.client.get(URL).send().and_then(|r| r.json::<Value>());
        match result {
            Ok(json) => {
                if let Some(t) = find_cpu_temp(&json) {
                    out.cpu_temp_c = Some(t);
                    out.cpu_temp_source = Some("LibreHardwareMonitor");
                }
            }
            Err(_) => self.disabled_until = Some(Instant::now() + Duration::from_secs(30)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prefers_cpu_package() {
        let tree = json!({"Text":"Sensor","Children":[{"Text":"Intel Core","Children":[
            {"Text":"Temperatures","Children":[
                {"Text":"CPU Core #1","Value":"41.0 °C"},
                {"Text":"CPU Package","Value":"47.5 °C"}
            ]},
            {"Text":"Load","Children":[{"Text":"CPU Package","Value":"12.0 %"}]}
        ]}]});
        assert_eq!(find_cpu_temp(&tree), Some(47.5));
    }

    #[test]
    fn none_without_temps() {
        assert_eq!(find_cpu_temp(&json!({"Text":"x","Children":[]})), None);
    }
}
