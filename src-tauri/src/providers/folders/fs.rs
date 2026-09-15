//! 폴더 위젯 파일 시스템 연산.
//!
//! 외부 자원(디스크)은 `FolderFs` 트레이트 뒤에 둔다. 테스트는 `tempfile` 로 실제 임시
//! 디렉터리에 대해 동작을 검증한다(순수 함수만으로는 검증하기 어려운 OS 동작이므로).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Win32 ERROR_NOT_SAME_DEVICE — 다른 볼륨으로 이동할 때 `rename` 이 이 코드로 실패한다.
const ERROR_NOT_SAME_DEVICE: i32 = 17;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// 표시용 이름 — `.lnk`/`.url` 확장자는 숨긴다 (경로 자체는 그대로 유지).
pub fn display_name(name: &str) -> String {
    for ext in [".lnk", ".url"] {
        if name.len() > ext.len() && name.to_lowercase().ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name.to_string()
}

pub trait FolderFs: Send + Sync {
    fn list(&self, dir: &Path) -> io::Result<Vec<RawEntry>>;
    fn move_into(&self, dir: &Path, src: &Path) -> io::Result<PathBuf>;
    fn copy_into(&self, dir: &Path, src: &Path) -> io::Result<PathBuf>;
}

pub struct RealFs;

impl RealFs {
    /// `dir` 안에서 `name` 과 충돌하지 않는 이름을 찾는다 (`foo.txt` → `foo (2).txt`).
    pub fn unique_name(dir: &Path, name: &str) -> String {
        if !dir.join(name).exists() {
            return name.to_string();
        }
        let path = Path::new(name);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
        let ext = path.extension().and_then(|s| s.to_str());
        let mut n = 2;
        loop {
            let candidate = match ext {
                Some(e) => format!("{stem} ({n}).{e}"),
                None => format!("{stem} ({n})"),
            };
            if !dir.join(&candidate).exists() {
                return candidate;
            }
            n += 1;
        }
    }
}

fn copy_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dest).map(|_| ())
    }
}

impl FolderFs for RealFs {
    fn list(&self, dir: &Path) -> io::Result<Vec<RawEntry>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            let meta = entry.metadata()?;
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::fs::MetadataExt;
                const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
                if meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 {
                    continue;
                }
            }
            out.push(RawEntry { name, path: entry.path(), is_dir: meta.is_dir() });
        }
        // 폴더 먼저, 그다음 이름순(대소문자 무시)
        out.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
        Ok(out)
    }

    fn move_into(&self, dir: &Path, src: &Path) -> io::Result<PathBuf> {
        let name = src
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?
            .to_string_lossy()
            .to_string();
        let dest = dir.join(Self::unique_name(dir, &name));
        match fs::rename(src, &dest) {
            Ok(()) => Ok(dest),
            // ERROR_NOT_SAME_DEVICE (다른 볼륨) 일 때만 복사+삭제로 대체한다. 그 외 오류는 그대로 전파.
            Err(e) if e.raw_os_error() == Some(ERROR_NOT_SAME_DEVICE) => {
                copy_recursive(src, &dest)?;
                let removed = if src.is_dir() { fs::remove_dir_all(src) } else { fs::remove_file(src) };
                match removed {
                    Ok(()) => Ok(dest),
                    Err(remove_err) => Err(io::Error::new(
                        remove_err.kind(),
                        format!("복사는 완료됐지만 원본을 삭제하지 못했습니다: {remove_err}"),
                    )),
                }
            }
            Err(e) => Err(e),
        }
    }

    fn copy_into(&self, dir: &Path, src: &Path) -> io::Result<PathBuf> {
        let name = src
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?
            .to_string_lossy()
            .to_string();
        let dest = dir.join(Self::unique_name(dir, &name));
        copy_recursive(src, &dest)?;
        Ok(dest)
    }
}

/// `path` 가 `dir` 의 직계 자식인지 검증한다 (경로 조작 방지). 둘 다 canonicalize 해서 비교한다.
pub fn is_direct_child(dir: &Path, path: &Path) -> io::Result<bool> {
    let dir_c = fs::canonicalize(dir)?;
    let path_c = fs::canonicalize(path)?;
    Ok(path_c.parent() == Some(dir_c.as_path()))
}

/// `dir` 로 `src` 를 옮기려는 요청을 검증한다.
/// - `dir` 이 `src` 자신이거나 `src` 의 하위 경로면 오류(자기 자신의 하위 트리로 옮기는 것 방지).
/// - `src` 가 이미 `dir` 의 직계 자식이면 `Ok(true)` — 호출자는 건너뛰어야 한다.
pub fn validate_move_target(dir: &Path, src: &Path) -> io::Result<bool> {
    let dir_c = fs::canonicalize(dir)?;
    let src_c = fs::canonicalize(src)?;
    if dir_c == src_c || dir_c.starts_with(&src_c) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "대상 폴더가 옮기려는 폴더 자신이거나 그 하위 경로입니다",
        ));
    }
    Ok(src_c.parent() == Some(dir_c.as_path()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn list_sorts_dirs_first_then_name_case_insensitive() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("A")).unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let names: Vec<_> = RealFs.list(tmp.path()).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["A", "a.txt", "b.txt"]);
    }

    #[test]
    fn list_skips_hidden_and_desktop_ini() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("desktop.ini"), "").unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();
        fs::write(tmp.path().join("visible.txt"), "").unwrap();
        let list = RealFs.list(tmp.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "visible.txt");
    }

    #[test]
    fn unique_name_appends_incrementing_suffix_on_collision() {
        let tmp = tempdir().unwrap();
        assert_eq!(RealFs::unique_name(tmp.path(), "foo.txt"), "foo.txt");
        fs::write(tmp.path().join("foo.txt"), "").unwrap();
        assert_eq!(RealFs::unique_name(tmp.path(), "foo.txt"), "foo (2).txt");
        fs::write(tmp.path().join("foo (2).txt"), "").unwrap();
        assert_eq!(RealFs::unique_name(tmp.path(), "foo.txt"), "foo (3).txt");
    }

    #[test]
    fn move_into_moves_file_and_removes_source() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let src = src_dir.path().join("file.txt");
        fs::write(&src, "hi").unwrap();
        let dest = RealFs.move_into(dest_dir.path(), &src).unwrap();
        assert!(!src.exists());
        assert_eq!(fs::read_to_string(&dest).unwrap(), "hi");
    }

    #[test]
    fn copy_into_keeps_source() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let src = src_dir.path().join("file.txt");
        fs::write(&src, "hi").unwrap();
        let dest = RealFs.copy_into(dest_dir.path(), &src).unwrap();
        assert!(src.exists());
        assert_eq!(fs::read_to_string(&dest).unwrap(), "hi");
    }

    #[test]
    fn is_direct_child_true_for_child_false_for_grandchild() {
        let tmp = tempdir().unwrap();
        let child = tmp.path().join("child.txt");
        fs::write(&child, "").unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let grandchild = sub.join("g.txt");
        fs::write(&grandchild, "").unwrap();
        assert!(is_direct_child(tmp.path(), &child).unwrap());
        assert!(!is_direct_child(tmp.path(), &grandchild).unwrap());
    }

    #[test]
    fn display_name_hides_lnk_and_url_extension() {
        assert_eq!(display_name("메모장.lnk"), "메모장");
        assert_eq!(display_name("사이트.URL"), "사이트");
        assert_eq!(display_name("readme.txt"), "readme.txt");
    }

    #[test]
    fn validate_move_target_rejects_moving_into_own_subtree() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        fs::create_dir(&parent).unwrap();
        let child_dir = parent.join("child");
        fs::create_dir(&child_dir).unwrap();
        // parent 를 그 자신의 하위 디렉터리(child_dir)로 옮기려는 시도는 거부돼야 한다.
        let err = validate_move_target(&child_dir, &parent).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn validate_move_target_rejects_dir_equal_to_src() {
        let tmp = tempdir().unwrap();
        let err = validate_move_target(tmp.path(), tmp.path()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn validate_move_target_true_when_already_direct_child() {
        let tmp = tempdir().unwrap();
        let file = tmp.path().join("f.txt");
        fs::write(&file, "").unwrap();
        assert!(validate_move_target(tmp.path(), &file).unwrap());
    }

    #[test]
    fn validate_move_target_false_for_unrelated_paths() {
        let dir = tempdir().unwrap();
        let src_dir = tempdir().unwrap();
        let src = src_dir.path().join("f.txt");
        fs::write(&src, "").unwrap();
        assert!(!validate_move_target(dir.path(), &src).unwrap());
    }
}
