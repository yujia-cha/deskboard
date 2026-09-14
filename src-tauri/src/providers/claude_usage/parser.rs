//! Claude Code 트랜스크립트(`~/.claude/projects/**/*.jsonl`) 증분 파서.
//!
//! 레코드 형식(관찰): `{"type":"assistant","timestamp":"...","requestId":"req_..",
//!   "message":{"id":"msg_..","model":"claude-...","usage":{input_tokens, output_tokens,
//!   cache_creation_input_tokens, cache_read_input_tokens, cache_creation:{ephemeral_5m_input_tokens, ephemeral_1h_input_tokens}}}}`
//! 같은 응답이 스트리밍 중 여러 번 기록되므로 `message.id:requestId` 로 중복 제거한다.

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
    /// 중복 제거 키. `message.id` 가 없으면 None (중복 제거 불가, 그대로 집계).
    pub key: Option<String>,
    /// 세션 id (파일명 기준). 현재 세션 집계용.
    pub session: String,
}

/// JSONL 한 줄 → 사용량 레코드. 사용량이 없는 줄(user, summary 등)은 None.
pub fn parse_line(line: &str, session: &str) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let msg = v.get("message")?;
    let usage = msg.get("usage")?;
    let model = msg.get("model").and_then(Value::as_str).unwrap_or("").to_string();
    if model.is_empty() || model == "<synthetic>" {
        return None;
    }
    let timestamp = v
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))?;
    let n = |o: &Value, k: &str| o.get(k).and_then(Value::as_u64).unwrap_or(0);
    let total_cache_write = n(usage, "cache_creation_input_tokens");
    let (w5, w1) = match usage.get("cache_creation") {
        Some(cc) => (n(cc, "ephemeral_5m_input_tokens"), n(cc, "ephemeral_1h_input_tokens")),
        None => (total_cache_write, 0),
    };
    // 세부 내역이 없거나 합이 안 맞으면 총량을 5분 캐시로 간주
    let (w5, w1) = if w5 + w1 == total_cache_write { (w5, w1) } else { (total_cache_write, 0) };
    let key = msg.get("id").and_then(Value::as_str).map(|id| {
        let req = v.get("requestId").and_then(Value::as_str).unwrap_or("");
        format!("{id}:{req}")
    });
    Some(UsageRecord {
        timestamp,
        model,
        input: n(usage, "input_tokens"),
        output: n(usage, "output_tokens"),
        cache_write_5m: w5,
        cache_write_1h: w1,
        cache_read: n(usage, "cache_read_input_tokens"),
        key,
        session: session.to_string(),
    })
}

/// 파일별 읽기 오프셋과 중복 키를 유지하는 증분 스캐너.
#[derive(Default)]
pub struct Scanner {
    offsets: HashMap<PathBuf, u64>,
    seen: HashSet<String>,
    pub records: Vec<UsageRecord>,
}

impl Scanner {
    /// 파일의 마지막 오프셋 이후 완성된 줄만 읽어 records 에 추가. 추가된 레코드 수를 돌려준다.
    pub fn scan_file(&mut self, path: &Path) -> std::io::Result<usize> {
        let mut f = File::open(path)?;
        let len = f.metadata()?.len();
        let start = *self.offsets.get(path).unwrap_or(&0);
        // 파일이 잘렸으면(재작성) 처음부터
        let start = if start > len { 0 } else { start };
        if start == len {
            return Ok(0);
        }
        f.seek(SeekFrom::Start(start))?;
        let session = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let mut reader = BufReader::new(f);
        let mut consumed = start;
        let mut added = 0;
        let mut buf = String::new();
        loop {
            buf.clear();
            let n = reader.read_line(&mut buf)?;
            if n == 0 {
                break;
            }
            if !buf.ends_with('\n') {
                break; // 아직 쓰는 중인 줄 — 다음에 다시
            }
            consumed += n as u64;
            if let Some(r) = parse_line(buf.trim_end(), &session) {
                if let Some(k) = &r.key {
                    if !self.seen.insert(k.clone()) {
                        continue;
                    }
                }
                self.records.push(r);
                added += 1;
            }
        }
        self.offsets.insert(path.to_path_buf(), consumed);
        Ok(added)
    }

    /// 디렉터리 아래 모든 `.jsonl` 을 스캔.
    pub fn scan_dir(&mut self, root: &Path) -> usize {
        let mut added = 0;
        for entry in walkdir::WalkDir::new(root).into_iter().filter_map(Result::ok) {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                match self.scan_file(p) {
                    Ok(n) => added += n,
                    Err(e) => log::debug!("skip {}: {e}", p.display()),
                }
            }
        }
        added
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const LINE: &str = r#"{"type":"assistant","timestamp":"2026-09-14T10:57:13.689Z","requestId":"req_1","message":{"id":"msg_1","model":"claude-fable-5-1","usage":{"input_tokens":2,"cache_creation_input_tokens":19663,"cache_read_input_tokens":44616,"output_tokens":285,"cache_creation":{"ephemeral_1h_input_tokens":19663,"ephemeral_5m_input_tokens":0}}}}"#;

    #[test]
    fn parses_assistant_usage() {
        let r = parse_line(LINE, "s1").unwrap();
        assert_eq!(r.model, "claude-fable-5-1");
        assert_eq!(r.input, 2);
        assert_eq!(r.output, 285);
        assert_eq!(r.cache_write_1h, 19663);
        assert_eq!(r.cache_write_5m, 0);
        assert_eq!(r.cache_read, 44616);
        assert_eq!(r.key.as_deref(), Some("msg_1:req_1"));
    }

    #[test]
    fn ignores_non_assistant_and_synthetic() {
        assert!(parse_line(r#"{"type":"user","message":{"usage":{}}}"#, "s").is_none());
        assert!(parse_line(r#"{"type":"assistant","timestamp":"2026-01-01T00:00:00Z","message":{"model":"<synthetic>","usage":{"input_tokens":1}}}"#, "s").is_none());
        assert!(parse_line("not json", "s").is_none());
    }

    #[test]
    fn dedupes_and_reads_incrementally() {
        let dir = std::env::temp_dir().join(format!("deskboard-parser-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("abc.jsonl");
        let mut f = File::create(&p).unwrap();
        writeln!(f, "{LINE}").unwrap();
        writeln!(f, "{LINE}").unwrap(); // 스트리밍 재기록 → 중복
        write!(f, "{{\"type\":\"assistant\"").unwrap(); // 미완성 줄

        let mut s = Scanner::default();
        assert_eq!(s.scan_file(&p).unwrap(), 1);
        assert_eq!(s.records[0].session, "abc");

        // 미완성 줄 완성 + 새 레코드
        writeln!(f, ",\"timestamp\":\"2026-09-14T11:00:00Z\",\"requestId\":\"req_2\",\"message\":{{\"id\":\"msg_2\",\"model\":\"claude-opus-5\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":5}}}}}}").unwrap();
        assert_eq!(s.scan_file(&p).unwrap(), 1);
        assert_eq!(s.records.len(), 2);
        assert_eq!(s.records[1].model, "claude-opus-5");
        assert_eq!(s.records[1].cache_write_5m, 0);
        assert_eq!(s.scan_file(&p).unwrap(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
