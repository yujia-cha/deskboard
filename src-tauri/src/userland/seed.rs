//! 데이터 폴더에 샘플·문서를 깔아 둔다.
//!
//! - `*.sample.*`, `widgets/README.md`, `widgets/widget.schema.json` 은 **매 시작 덮어쓴다** (사용자가 고칠 파일이 아니다).
//! - 예제 위젯 `widgets/_example-disk/`(JSX + command) · `widgets/_example-cpu/`(HTML) 은 **없을 때만** 만든다.
//! - 실제 설정 파일(`config.jsonc` 등)은 **절대 건드리지 않는다.**

use std::path::Path;

const ALWAYS: [(&str, &str); 5] = [
    ("config.sample.jsonc", include_str!("../../resources/userland/config.sample.jsonc")),
    ("strings.sample.jsonc", include_str!("../../resources/userland/strings.sample.jsonc")),
    ("user.sample.css", include_str!("../../resources/userland/user.sample.css")),
    ("widgets/README.md", include_str!("../../resources/userland/widgets/README.md")),
    ("widgets/widget.schema.json", include_str!("../../resources/userland/widgets/widget.schema.json")),
];

/// 예제 위젯 — 이름이 `_` 로 시작해 꺼져 있다. 사용자가 이름을 바꾸면 켜진다.
/// 한 번 만들고 나면 다시 쓰지 않는다 (사용자가 고치거나 지운 것을 되살리지 않는다).
const EXAMPLES: [(&str, &[(&str, &str)]); 2] = [
    ("widgets/_example-disk", &[
        ("widget.json", include_str!("../../resources/userland/widgets/_example-disk/widget.json")),
        ("index.jsx", include_str!("../../resources/userland/widgets/_example-disk/index.jsx")),
    ]),
    ("widgets/_example-cpu", &[
        ("widget.json", include_str!("../../resources/userland/widgets/_example-cpu/widget.json")),
        ("index.html", include_str!("../../resources/userland/widgets/_example-cpu/index.html")),
        ("style.css", include_str!("../../resources/userland/widgets/_example-cpu/style.css")),
        ("app.js", include_str!("../../resources/userland/widgets/_example-cpu/app.js")),
    ]),
];

/// 내용이 같으면 쓰지 않는다 (불필요한 파일 변경 알림을 막는다).
fn write_if_changed(path: &Path, content: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|cur| cur == content) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// 실패해도 앱은 계속 뜬다 — 경고만 남긴다.
pub fn run(data_dir: &Path) {
    for (rel, content) in ALWAYS {
        if let Err(e) = write_if_changed(&data_dir.join(rel), content) {
            log::warn!("샘플 파일을 쓰지 못했습니다 ({rel}): {e}");
        }
    }
    for (dir, files) in EXAMPLES {
        let example = data_dir.join(dir);
        if example.exists() {
            continue;
        }
        for (name, content) in files {
            if let Err(e) = write_if_changed(&example.join(name), content) {
                log::warn!("예제 위젯을 쓰지 못했습니다 ({dir}/{name}): {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn seeds_samples_and_example_without_touching_real_files() {
        let t = TempDir::new().unwrap();
        std::fs::write(t.path().join("config.jsonc"), "{ mine }").unwrap();
        std::fs::create_dir_all(t.path().join("widgets")).unwrap();
        std::fs::write(t.path().join("widgets/README.md"), "old").unwrap();
        run(t.path());
        assert_eq!(std::fs::read_to_string(t.path().join("config.jsonc")).unwrap(), "{ mine }");
        assert_ne!(std::fs::read_to_string(t.path().join("widgets/README.md")).unwrap(), "old");
        for f in ["config.sample.jsonc", "strings.sample.jsonc", "user.sample.css", "widgets/widget.schema.json"] {
            assert!(t.path().join(f).is_file(), "{f}");
        }
        assert!(t.path().join("widgets/_example-disk/widget.json").is_file());
        assert!(t.path().join("widgets/_example-disk/index.jsx").is_file());
        assert!(t.path().join("widgets/_example-cpu/index.html").is_file());
        assert!(t.path().join("widgets/_example-cpu/app.js").is_file());
    }

    #[test]
    fn example_is_created_only_once() {
        let t = TempDir::new().unwrap();
        run(t.path());
        let p = t.path().join("widgets/_example-disk/index.jsx");
        std::fs::write(&p, "// mine").unwrap();
        run(t.path());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "// mine");
        // 폴더를 지우면 다음 시작에 다시 만든다
        std::fs::remove_dir_all(t.path().join("widgets/_example-disk")).unwrap();
        run(t.path());
        assert!(p.is_file());
    }

    #[test]
    fn samples_are_valid_jsonc() {
        for (rel, content) in ALWAYS {
            if rel.ends_with(".jsonc") {
                let v: serde_json::Value = serde_json::from_str(&super::super::jsonc::strip(content)).unwrap_or_else(|e| panic!("{rel}: {e}"));
                assert!(v.is_object());
            }
        }
        let schema: serde_json::Value = serde_json::from_str(ALWAYS[4].1).unwrap();
        assert!(schema.is_object());
        for (dir, files) in EXAMPLES {
            for (name, c) in files.iter().filter(|(n, _)| n.ends_with(".json")) {
                serde_json::from_str::<serde_json::Value>(c).unwrap_or_else(|e| panic!("{dir}/{name}: {e}"));
            }
        }
    }
}
