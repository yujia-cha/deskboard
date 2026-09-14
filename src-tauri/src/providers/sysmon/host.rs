//! sysinfo 기반 CPU 사용률 / 메모리.

use super::{SensorSample, SensorSource};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

pub struct HostSource {
    sys: System,
    cpu_name: String,
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
        Self { sys, cpu_name }
    }
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
    }
}
