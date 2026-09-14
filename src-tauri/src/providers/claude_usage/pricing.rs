//! 모델별 단가표. 기본값은 바이너리에 포함된 `resources/pricing.json`,
//! 앱 데이터 폴더의 `pricing.json` 이 있으면 항목 단위로 덮어쓴다.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

const DEFAULT_JSON: &str = include_str!("../../../resources/pricing.json");

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
}

#[derive(Debug, Deserialize)]
struct PricingFile {
    models: HashMap<String, ModelPrice>,
}

#[derive(Debug, Clone, Default)]
pub struct Pricing {
    models: HashMap<String, ModelPrice>,
}

impl Pricing {
    pub fn builtin() -> Self {
        Self::from_json(DEFAULT_JSON).expect("bundled pricing.json is valid")
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        let f: PricingFile = serde_json::from_str(text)?;
        Ok(Self { models: f.models })
    }

    /// 내장 표 위에 사용자 파일을 병합한다. 파일이 없으면 내장 표 그대로.
    pub fn load_with_override(user_file: &Path) -> Self {
        let mut p = Self::builtin();
        if let Ok(text) = std::fs::read_to_string(user_file) {
            match Self::from_json(&text) {
                Ok(u) => p.models.extend(u.models),
                Err(e) => log::warn!("ignoring invalid {}: {e}", user_file.display()),
            }
        }
        p
    }

    /// 가장 긴 접두사가 일치하는 항목 (예: `claude-opus-5-20260401` → `claude-opus-5`).
    pub fn lookup(&self, model: &str) -> Option<ModelPrice> {
        self.models
            .iter()
            .filter(|(k, _)| model.starts_with(k.as_str()))
            .max_by_key(|(k, _)| k.len())
            .map(|(_, v)| *v)
    }
}

/// 토큰 수 → USD.
pub fn cost_usd(p: &ModelPrice, input: u64, output: u64, cache_w5m: u64, cache_w1h: u64, cache_read: u64) -> f64 {
    let m = 1_000_000.0;
    (input as f64 * p.input
        + output as f64 * p.output
        + cache_w5m as f64 * p.cache_write_5m
        + cache_w1h as f64 * p.cache_write_1h
        + cache_read as f64 * p.cache_read)
        / m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_prefix_wins() {
        let p = Pricing::builtin();
        // "claude-fable-5-1" 은 "claude-fable-5" 보다 길므로 우선
        assert_eq!(p.lookup("claude-fable-5-1").unwrap().cache_read, 0.25);
        assert_eq!(p.lookup("claude-fable-5-20260301").unwrap().cache_read, 1.0);
        assert_eq!(p.lookup("claude-opus-5-20260401").unwrap().input, 5.0);
        assert!(p.lookup("<synthetic>").is_none());
    }

    #[test]
    fn cost_math() {
        let p = ModelPrice { input: 10.0, output: 50.0, cache_write_5m: 12.5, cache_write_1h: 20.0, cache_read: 1.0 };
        let c = cost_usd(&p, 1_000_000, 0, 0, 0, 0);
        assert!((c - 10.0).abs() < 1e-9);
        let c = cost_usd(&p, 0, 100_000, 0, 0, 0);
        assert!((c - 5.0).abs() < 1e-9);
    }

    #[test]
    fn override_merges() {
        let dir = std::env::temp_dir().join(format!("deskboard-pricing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("pricing.json");
        std::fs::write(&f, r#"{"models":{"my-model":{"input":1,"output":2,"cache_write_5m":3,"cache_write_1h":4,"cache_read":5}}}"#).unwrap();
        let p = Pricing::load_with_override(&f);
        assert_eq!(p.lookup("my-model-x").unwrap().output, 2.0);
        assert!(p.lookup("claude-opus-5").is_some());
        let _ = std::fs::remove_dir_all(dir);
    }
}
