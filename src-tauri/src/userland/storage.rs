//! 위젯 인스턴스 저장소 — `userdata/<instanceId>.json`. 위젯 코드가 `useStorage` 로 쓴다.

use serde_json::Value;
use std::path::{Path, PathBuf};

const MAX_BYTES: usize = 256 * 1024;

/// `^[A-Za-z0-9-]{1,64}$` — 파일 이름이 되므로 경로 문자는 절대 못 들어온다.
pub fn valid_instance_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

fn path_of(dir: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_instance_id(id) {
        return Err("올바르지 않은 인스턴스 id 입니다".into());
    }
    Ok(dir.join(format!("{id}.json")))
}

pub fn get(dir: &Path, id: &str) -> Result<Option<Value>, String> {
    let p = path_of(dir, id)?;
    match std::fs::read_to_string(&p) {
        Ok(t) => serde_json::from_str(&t).map(Some).map_err(|e| format!("저장된 데이터가 깨져 있습니다: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("저장소를 읽지 못했습니다: {e}")),
    }
}

/// 임시 파일에 쓴 뒤 rename — 쓰는 도중 꺼져도 옛 값이 남는다.
pub fn set(dir: &Path, id: &str, value: &Value) -> Result<(), String> {
    let p = path_of(dir, id)?;
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    if text.len() > MAX_BYTES {
        return Err(format!("저장 데이터가 너무 큽니다 ({}KB, 최대 256KB)", text.len() / 1024));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("저장 폴더를 만들지 못했습니다: {e}"))?;
    let tmp = dir.join(format!("{id}.json.tmp"));
    std::fs::write(&tmp, text).map_err(|e| format!("저장하지 못했습니다: {e}"))?;
    std::fs::rename(&tmp, &p).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("저장하지 못했습니다: {e}")
    })
}

pub fn delete(dir: &Path, id: &str) -> Result<(), String> {
    let p = path_of(dir, id)?;
    match std::fs::remove_file(&p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("삭제하지 못했습니다: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn id_validation() {
        assert!(valid_instance_id("a1-B2"));
        assert!(valid_instance_id(&"a".repeat(64)));
        for bad in ["", "a/b", "..", "a.b", "a_b", "a b", "한글", &"a".repeat(65)] {
            assert!(!valid_instance_id(bad), "{bad}");
        }
    }

    #[test]
    fn roundtrip_and_delete() {
        let t = TempDir::new().unwrap();
        let d = t.path().join("userdata");
        assert_eq!(get(&d, "x1").unwrap(), None);
        set(&d, "x1", &json!({"a": [1, 2], "b": "한글"})).unwrap();
        assert_eq!(get(&d, "x1").unwrap().unwrap()["b"], "한글");
        set(&d, "x1", &json!(5)).unwrap();
        assert_eq!(get(&d, "x1").unwrap().unwrap(), 5);
        assert!(!d.join("x1.json.tmp").exists());
        delete(&d, "x1").unwrap();
        assert_eq!(get(&d, "x1").unwrap(), None);
        delete(&d, "x1").unwrap(); // 없어도 OK
    }

    #[test]
    fn rejects_bad_id_and_oversize() {
        let t = TempDir::new().unwrap();
        assert!(set(t.path(), "../x", &json!(1)).is_err());
        assert!(get(t.path(), "a/b").is_err());
        assert!(delete(t.path(), "").is_err());
        let big = json!("a".repeat(256 * 1024));
        assert!(set(t.path(), "big", &big).is_err());
        let ok = json!("a".repeat(256 * 1024 - 2));
        assert!(set(t.path(), "ok", &ok).is_ok());
    }
}
