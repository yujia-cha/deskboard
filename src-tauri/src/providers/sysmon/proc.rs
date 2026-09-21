//! CPU 를 가장 많이 쓰는 프로세스 상위 N개.
//!
//! 프로세스 목록 갱신은 CPU/메모리 읽기보다 훨씬 비싸다. 위젯이 이 정보를 실제로 보여줄 때만
//! 수집한다 (`sysmon_set_detail`). 꺼져 있으면 `sample` 이 즉시 반환한다.

use super::{ProcSample, SensorSample, SensorSource};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

/// 위젯이 상위 프로세스를 보여주는 중인가.
static ENABLED: AtomicBool = AtomicBool::new(false);
/// 보여줄 개수.
static COUNT: AtomicUsize = AtomicUsize::new(5);

/// 프론트가 위젯 설정에 맞춰 켜고 끈다 (Spotify 의 `spotify_set_active` 와 같은 패턴).
#[tauri::command]
pub fn sysmon_set_detail(enabled: bool, count: Option<u32>) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if let Some(n) = count {
        COUNT.store((n as usize).clamp(1, 12), Ordering::Relaxed);
    }
}

pub struct ProcSource {
    sys: System,
    /// 코어 수 — sysinfo 의 프로세스 CPU 는 코어 합산이라 100% 기준으로 되돌린다.
    cores: usize,
}

impl ProcSource {
    pub fn new() -> Self {
        Self {
            sys: System::new_with_specifics(RefreshKind::nothing()),
            cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        }
    }
}

/// CPU 내림차순 상위 `n` 개. 동률이면 이름순이라 순서가 매 틱 흔들리지 않는다.
pub fn top_n(mut procs: Vec<ProcSample>, n: usize) -> Vec<ProcSample> {
    procs.sort_by(|a, b| {
        b.cpu
            .partial_cmp(&a.cpu)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    procs.truncate(n);
    procs
}

/// 여러 코어에 걸친 사용률을 전체 대비 %로 바꾼다 (sysinfo 는 코어당 100% 를 더해서 준다).
pub fn normalize_cpu(raw: f32, cores: usize) -> f32 {
    if cores == 0 {
        return raw;
    }
    (raw / cores as f32).clamp(0.0, 100.0)
}

impl SensorSource for ProcSource {
    fn name(&self) -> &'static str {
        "processes"
    }

    fn sample(&mut self, out: &mut SensorSample) {
        if !ENABLED.load(Ordering::Relaxed) {
            out.top = Vec::new();
            return;
        }
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let procs = self
            .sys
            .processes()
            .values()
            .map(|p| ProcSample {
                name: p.name().to_string_lossy().to_string(),
                cpu: normalize_cpu(p.cpu_usage(), self.cores),
                mem: p.memory(),
            })
            // 유휴 프로세스 수백 개를 다 보낼 필요는 없다
            .filter(|p| p.cpu > 0.05)
            .collect();
        out.top = top_n(procs, COUNT.load(Ordering::Relaxed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, cpu: f32) -> ProcSample {
        ProcSample { name: name.into(), cpu, mem: 0 }
    }

    #[test]
    fn keeps_the_busiest_processes() {
        let got = top_n(vec![p("a", 1.0), p("b", 30.0), p("c", 10.0)], 2);
        assert_eq!(got.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["b", "c"]);
    }

    #[test]
    fn ties_break_by_name_so_the_list_does_not_jitter() {
        let got = top_n(vec![p("zeta", 5.0), p("alpha", 5.0), p("mid", 5.0)], 3);
        assert_eq!(got.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["alpha", "mid", "zeta"]);
    }

    #[test]
    fn asking_for_more_than_exists_returns_what_there_is() {
        assert_eq!(top_n(vec![p("a", 1.0)], 5).len(), 1);
        assert_eq!(top_n(vec![], 5).len(), 0);
    }

    #[test]
    fn cpu_is_scaled_back_to_a_whole_machine_percentage() {
        // 8코어에서 한 프로세스가 4코어를 꽉 쓰면 sysinfo 는 400% 를 준다 → 50%
        assert_eq!(normalize_cpu(400.0, 8), 50.0);
        assert_eq!(normalize_cpu(800.0, 8), 100.0);
        assert_eq!(normalize_cpu(0.0, 8), 0.0);
    }

    #[test]
    fn normalize_never_exceeds_100_or_divides_by_zero() {
        assert_eq!(normalize_cpu(9999.0, 4), 100.0);
        assert_eq!(normalize_cpu(50.0, 0), 50.0);
    }
}
