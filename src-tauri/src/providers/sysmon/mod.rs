//! 시스템 모니터 — CPU/GPU/RAM 사용량과 온도.
//!
//! `SensorSource` 구현체를 조합해 2초마다 `sysmon://update` 를 보낸다.
//! 소스가 없거나 실패하면 해당 필드는 `None` 으로 남고 UI 가 숨긴다.

mod host;
mod lhm;
mod net;
mod nvidia;
// 커맨드가 이 안에 있다 — `#[tauri::command]` 가 만드는 보조 아이템은 재수출로 따라오지
// 않으므로 모듈째 공개하고 `providers::sysmon::proc::sysmon_set_detail` 로 등록한다.
pub mod proc;

use super::Provider;
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Default)]
pub struct GpuSample {
    pub name: String,
    pub temp_c: Option<f32>,
    pub usage: Option<f32>,
    pub mem_used_mb: Option<u64>,
    pub mem_total_mb: Option<u64>,
    pub power_w: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DiskSample {
    /// `C:` 형태로 정규화된 마운트
    pub mount: String,
    pub name: String,
    pub used: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct NetSample {
    /// 가장 바쁜 인터페이스 이름 (툴팁용)
    pub iface: String,
    pub up_bps: u64,
    pub down_bps: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProcSample {
    pub name: String,
    /// 전체 CPU 대비 % (코어 합산이 아니다)
    pub cpu: f32,
    /// 사용 중인 메모리 바이트
    pub mem: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SensorSample {
    pub cpu_name: String,
    pub cpu_usage: f32,
    pub cpu_cores: Vec<f32>,
    pub cpu_temp_c: Option<f32>,
    /// CPU 온도 출처 설명 (UI 툴팁용). 없으면 None.
    pub cpu_temp_source: Option<&'static str>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub gpu: Option<GpuSample>,
    pub disks: Vec<DiskSample>,
    pub net: Option<NetSample>,
    /// CPU 상위 프로세스. 위젯이 요청할 때만 채워진다 (`sysmon_set_detail`).
    pub top: Vec<ProcSample>,
}

/// 센서 소스 하나. `sample` 은 자기 필드만 채운다.
pub trait SensorSource: Send {
    fn name(&self) -> &'static str;
    fn sample(&mut self, out: &mut SensorSample);
}

pub struct SysmonProvider;

impl Provider for SysmonProvider {
    fn id(&self) -> &'static str {
        "sysmon"
    }

    fn start(&self, app: AppHandle) {
        std::thread::Builder::new()
            .name("sysmon".into())
            .spawn(move || run(app))
            .expect("spawn sysmon thread");
    }
}

fn run(app: AppHandle) {
    let mut sources: Vec<Box<dyn SensorSource>> = vec![
        Box::new(host::HostSource::new()),
        Box::new(net::NetSource::new()),
        Box::new(proc::ProcSource::new()),
        Box::new(nvidia::NvidiaSmiSource::new()),
        Box::new(lhm::LhmSource::new()),
    ];
    log::info!("sysmon sources: {:?}", sources.iter().map(|s| s.name()).collect::<Vec<_>>());
    let interval = Duration::from_secs(2);
    loop {
        let started = Instant::now();
        let mut out = SensorSample::default();
        for s in sources.iter_mut() {
            s.sample(&mut out);
        }
        if let Err(e) = app.emit("sysmon://update", &out) {
            log::warn!("sysmon emit failed: {e}");
        }
        std::thread::sleep(interval.saturating_sub(started.elapsed()));
    }
}
