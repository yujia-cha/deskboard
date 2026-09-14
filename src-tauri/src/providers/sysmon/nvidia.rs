//! `nvidia-smi` 를 호출해 GPU 온도/사용률/VRAM/전력을 읽는다.
//! 실행 파일이 없거나 실패하면 비활성화되고 60초마다 재시도한다.

use super::{GpuSample, SensorSample, SensorSource};
use std::process::Command;
use std::time::{Duration, Instant};

const QUERY: &str = "name,temperature.gpu,utilization.gpu,memory.used,memory.total,power.draw";

pub struct NvidiaSmiSource {
    disabled_until: Option<Instant>,
}

impl NvidiaSmiSource {
    pub fn new() -> Self {
        Self { disabled_until: None }
    }

    fn query() -> Option<GpuSample> {
        let mut cmd = Command::new("nvidia-smi");
        cmd.args([&format!("--query-gpu={QUERY}"), "--format=csv,noheader,nounits"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let output = cmd.output().ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        parse_line(text.lines().next()?)
    }
}

/// `name, temp, util, mem.used, mem.total, power` 한 줄 파싱. 값이 `[N/A]` 면 None.
fn parse_line(line: &str) -> Option<GpuSample> {
    let cols: Vec<&str> = line.split(',').map(str::trim).collect();
    if cols.len() < 5 {
        return None;
    }
    let num = |i: usize| cols.get(i).and_then(|v| v.parse::<f32>().ok());
    Some(GpuSample {
        name: cols[0].to_string(),
        temp_c: num(1),
        usage: num(2),
        mem_used_mb: num(3).map(|v| v as u64),
        mem_total_mb: num(4).map(|v| v as u64),
        power_w: num(5),
    })
}

impl SensorSource for NvidiaSmiSource {
    fn name(&self) -> &'static str {
        "nvidia-smi"
    }

    fn sample(&mut self, out: &mut SensorSample) {
        if let Some(until) = self.disabled_until {
            if Instant::now() < until {
                return;
            }
            self.disabled_until = None;
        }
        match Self::query() {
            Some(gpu) => out.gpu = Some(gpu),
            None => {
                log::debug!("nvidia-smi unavailable; retry in 60s");
                self.disabled_until = Some(Instant::now() + Duration::from_secs(60));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_csv_line() {
        let g = parse_line("NVIDIA GeForce RTX 5060 Ti, 55, 12, 2698, 8151, 48.21").unwrap();
        assert_eq!(g.name, "NVIDIA GeForce RTX 5060 Ti");
        assert_eq!(g.temp_c, Some(55.0));
        assert_eq!(g.usage, Some(12.0));
        assert_eq!(g.mem_used_mb, Some(2698));
        assert_eq!(g.mem_total_mb, Some(8151));
        assert_eq!(g.power_w, Some(48.21));
    }

    #[test]
    fn na_values_become_none() {
        let g = parse_line("GPU, [N/A], 3, 10, 100, [N/A]").unwrap();
        assert_eq!(g.temp_c, None);
        assert_eq!(g.power_w, None);
    }
}
