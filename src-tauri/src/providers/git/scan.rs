//! 저장소 찾기 + 상태 읽기.

use git2::{BranchType, Repository, StatusOptions};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct RepoStatus {
    /// 폴더 이름
    pub name: String,
    pub path: String,
    /// 현재 브랜치. 분리된 HEAD 면 짧은 커밋 해시.
    pub branch: String,
    /// 커밋되지 않은 변경 파일 수 (스테이지 + 워킹트리, untracked 포함)
    pub dirty: usize,
    /// upstream 대비 앞선 커밋 수. upstream 이 없으면 0.
    pub ahead: usize,
    pub behind: usize,
    /// upstream 추적 브랜치가 있는지
    pub tracked: bool,
    /// 마지막 커밋 시각 (ISO-8601 로컬). 커밋이 없으면 빈 문자열.
    pub last_commit: String,
}

/// 손댈 것이 있는가 — 위젯이 강조할지 정하는 데 쓴다.
pub fn needs_attention(s: &RepoStatus) -> bool {
    s.dirty > 0 || s.ahead > 0 || s.behind > 0
}

/// `root` 아래에서 `.git` 을 가진 폴더를 찾는다 (깊이 `depth` 까지).
///
/// 저장소를 찾으면 그 안으로는 더 내려가지 않는다 — 서브모듈이나 `node_modules` 안의
/// 저장소까지 긁으면 목록이 쓸모없어진다.
pub fn find_repos(root: &Path, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return out;
    }
    // root 자체가 저장소일 수도 있다
    if root.join(".git").exists() {
        out.push(root.to_path_buf());
        return out;
    }
    walk(root, depth, &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = e.file_name();
        let name = name.to_string_lossy();
        // 들어가 봐야 시간만 버리는 폴더들
        if name.starts_with('.') || matches!(name.as_ref(), "node_modules" | "target" | "dist" | "build" | "vendor") {
            continue;
        }
        if p.join(".git").exists() {
            out.push(p);
            continue; // 저장소 안으로는 내려가지 않는다
        }
        walk(&p, depth - 1, out);
    }
}

/// 저장소 하나의 상태.
pub fn repo_status(path: &Path) -> Result<RepoStatus, git2::Error> {
    let repo = Repository::open(path)?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let mut opts = StatusOptions::new();
    opts.include_untracked(true).include_ignored(false).recurse_untracked_dirs(false);
    let dirty = repo.statuses(Some(&mut opts)).map(|s| s.len()).unwrap_or(0);

    let head = repo.head().ok();
    let branch = match &head {
        Some(h) if h.is_branch() => h.shorthand().unwrap_or("?").to_string(),
        // 분리된 HEAD — 짧은 해시로 보여준다
        Some(h) => h
            .target()
            .map(|oid| oid.to_string()[..7].to_string())
            .unwrap_or_else(|| "detached".into()),
        None => "(빈 저장소)".into(),
    };

    let last_commit = head
        .as_ref()
        .and_then(|h| h.peel_to_commit().ok())
        .map(|c| {
            let secs = c.time().seconds();
            chrono::DateTime::from_timestamp(secs, 0)
                .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%dT%H:%M:%S").to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    // upstream 대비 앞/뒤. fetch 는 하지 않으므로 마지막으로 받아온 시점 기준이다.
    let (mut ahead, mut behind, mut tracked) = (0, 0, false);
    if let (Some(h), true) = (&head, head.as_ref().map(|h| h.is_branch()).unwrap_or(false)) {
        if let Some(short) = h.shorthand() {
            if let Ok(local) = repo.find_branch(short, BranchType::Local) {
                if let Ok(up) = local.upstream() {
                    tracked = true;
                    if let (Some(a), Some(b)) = (h.target(), up.get().target()) {
                        if let Ok((x, y)) = repo.graph_ahead_behind(a, b) {
                            ahead = x;
                            behind = y;
                        }
                    }
                }
            }
        }
    }

    Ok(RepoStatus {
        name,
        path: path.to_string_lossy().to_string(),
        branch,
        dirty,
        ahead,
        behind,
        tracked,
        last_commit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(dir: &Path, rel: &str) {
        std::fs::create_dir_all(dir.join(rel)).unwrap();
    }
    fn mk_repo(dir: &Path, rel: &str) {
        mk(dir, rel);
        std::fs::create_dir_all(dir.join(rel).join(".git")).unwrap();
    }

    #[test]
    fn finds_repositories_one_level_down() {
        let t = tempfile::tempdir().unwrap();
        mk_repo(t.path(), "alpha");
        mk_repo(t.path(), "beta");
        mk(t.path(), "not-a-repo");
        let got = find_repos(t.path(), 2);
        let names: Vec<_> = got.iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn finds_repositories_two_levels_down() {
        let t = tempfile::tempdir().unwrap();
        mk_repo(t.path(), "group/inner");
        assert_eq!(find_repos(t.path(), 2).len(), 1);
        // 깊이가 모자라면 못 찾는다
        assert_eq!(find_repos(t.path(), 1).len(), 0);
    }

    #[test]
    fn a_root_that_is_itself_a_repository_is_returned_alone() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join(".git")).unwrap();
        mk_repo(t.path(), "nested");
        let got = find_repos(t.path(), 3);
        assert_eq!(got, vec![t.path().to_path_buf()]);
    }

    #[test]
    fn does_not_descend_into_a_repository() {
        // 서브모듈이나 저장소 안의 저장소까지 긁으면 목록이 쓸모없어진다
        let t = tempfile::tempdir().unwrap();
        mk_repo(t.path(), "outer");
        mk_repo(t.path(), "outer/inner");
        let got = find_repos(t.path(), 5);
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with("outer"));
    }

    #[test]
    fn skips_noise_directories() {
        let t = tempfile::tempdir().unwrap();
        mk_repo(t.path(), "node_modules/pkg");
        mk_repo(t.path(), "target/debug");
        mk_repo(t.path(), ".cache/thing");
        mk_repo(t.path(), "real");
        let got = find_repos(t.path(), 4);
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with("real"));
    }

    #[test]
    fn a_missing_root_yields_nothing_rather_than_failing() {
        assert_eq!(find_repos(Path::new("C:/definitely/not/here"), 2).len(), 0);
    }

    #[test]
    fn attention_is_needed_only_when_something_is_pending() {
        let clean = RepoStatus { name: "a".into(), ..Default::default() };
        assert!(!needs_attention(&clean));
        assert!(needs_attention(&RepoStatus { dirty: 1, ..clean.clone() }));
        assert!(needs_attention(&RepoStatus { ahead: 2, ..clean.clone() }));
        assert!(needs_attention(&RepoStatus { behind: 3, ..clean.clone() }));
    }

    #[test]
    fn reading_a_non_repository_is_an_error_not_a_panic() {
        let t = tempfile::tempdir().unwrap();
        assert!(repo_status(t.path()).is_err());
    }
}
