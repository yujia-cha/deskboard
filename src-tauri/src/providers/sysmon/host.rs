//! sysinfo 기반 CPU 사용률 / 메모리 / 디스크.

use super::{DiskSample, SensorSample, SensorSource};
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};

pub struct HostSource {
    sys: System,
    disks: Disks,
    cpu_name: String,
    tick: u32,
}

impl HostSource {
    pub fn new() -> Self {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
                .with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        sys.refresh_cpu_usage();
        let cpu_name = sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
        Self { sys, disks: Disks::new_with_refreshed_list(), cpu_name, tick: 0 }
    }
}

/// 드라이브 표기 정규화: `c:`, `C:\`, `C` → `C:`
pub fn normalize_mount(s: &str) -> String {
    let t = s.trim().trim_end_matches(['\\', '/']).to_uppercase();
    if t.len() == 1 && t.chars().all(|c| c.is_ascii_alphabetic()) { format!("{t}:") } else { t }
}

impl SensorSource for HostSource {
    fn name(&self) -> &'static str {
        "sysinfo"
    }

    fn sample(&mut self, out: &mut SensorSample) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        out.cpu_name = self.cpu_name.clone();
        out.cpu_usage = self.sys.global_cpu_usage();
        out.cpu_cores = self.sys.cpus().iter().map(|c| c.cpu_usage()).collect();
        out.mem_used = self.sys.used_memory();
        out.mem_total = self.sys.total_memory();

        // 디스크는 10초(5샘플)마다
        if self.tick % 5 == 0 {
            self.disks.refresh(true);
        }
        self.tick = self.tick.wrapping_add(1);
        out.disks = self
            .disks
            .list()
            .iter()
            .filter(|d| !d.is_removable() && d.total_space() > 0)
            .map(|d| DiskSample {
                mount: normalize_mount(&d.mount_point().to_string_lossy()),
                name: d.name().to_string_lossy().to_string(),
                used: d.total_space().saturating_sub(d.available_space()),
                total: d.total_space(),
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_drive_letters() {
        assert_eq!(normalize_mount("C:\\"), "C:");
        assert_eq!(normalize_mount("c:"), "C:");
        assert_eq!(normalize_mount(" d "), "D:");
        assert_eq!(normalize_mount("/"), "");
    }
}
