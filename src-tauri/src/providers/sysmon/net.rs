//! 네트워크 처리량 — 루프백을 뺀 모든 인터페이스의 합.
//!
//! sysinfo 는 "마지막 refresh 이후 누적 바이트"를 준다. 초당 속도로 바꾸려면 경과 시간으로
//! 나눠야 하는데, 폴링 간격이 늘 정확하지는 않으므로(스레드 sleep) 실제 경과를 재서 쓴다.

use super::{NetSample, SensorSample, SensorSource};
use std::time::Instant;
use sysinfo::Networks;

pub struct NetSource {
    nets: Networks,
    last: Instant,
}

impl NetSource {
    pub fn new() -> Self {
        Self { nets: Networks::new_with_refreshed_list(), last: Instant::now() }
    }
}

/// 루프백·가상 인터페이스로 보이는 이름인지. 이런 것까지 더하면 실제 트래픽이 부풀려진다.
pub fn is_virtual(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("loopback")
        || n.starts_with("lo")
        || n.contains("pseudo-interface")
        || n.contains("vethernet")
        || n.contains("vmware")
        || n.contains("virtualbox")
        || n.contains("hyper-v")
}

/// 누적 바이트와 경과 시간으로 초당 속도를 낸다.
/// 경과가 0 이거나 음수면 0 을 돌려준다 (0 으로 나누지 않는다).
pub fn bytes_per_sec(bytes: u64, secs: f64) -> u64 {
    if secs <= 0.0 || !secs.is_finite() {
        return 0;
    }
    (bytes as f64 / secs).round().max(0.0) as u64
}

impl SensorSource for NetSource {
    fn name(&self) -> &'static str {
        "network"
    }

    fn sample(&mut self, out: &mut SensorSample) {
        self.nets.refresh(true);
        let secs = self.last.elapsed().as_secs_f64();
        self.last = Instant::now();

        let mut down = 0u64;
        let mut up = 0u64;
        let mut busiest: Option<(&str, u64)> = None;
        for (name, data) in self.nets.iter() {
            if is_virtual(name) {
                continue;
            }
            let (rx, tx) = (data.received(), data.transmitted());
            down = down.saturating_add(rx);
            up = up.saturating_add(tx);
            let total = rx.saturating_add(tx);
            if busiest.map_or(true, |(_, b)| total > b) {
                busiest = Some((name, total));
            }
        }
        out.net = Some(NetSample {
            iface: busiest.map(|(n, _)| n.to_string()).unwrap_or_default(),
            down_bps: bytes_per_sec(down, secs),
            up_bps: bytes_per_sec(up, secs),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_divide_by_the_elapsed_time() {
        assert_eq!(bytes_per_sec(2000, 2.0), 1000);
        assert_eq!(bytes_per_sec(1500, 1.5), 1000);
        assert_eq!(bytes_per_sec(0, 2.0), 0);
    }

    #[test]
    fn a_zero_or_bogus_interval_never_divides_by_zero() {
        assert_eq!(bytes_per_sec(1000, 0.0), 0);
        assert_eq!(bytes_per_sec(1000, -1.0), 0);
        assert_eq!(bytes_per_sec(1000, f64::NAN), 0);
        assert_eq!(bytes_per_sec(1000, f64::INFINITY), 0);
    }

    #[test]
    fn virtual_adapters_are_excluded() {
        for n in ["Loopback Pseudo-Interface 1", "lo", "vEthernet (WSL)", "VMware Network Adapter VMnet1"] {
            assert!(is_virtual(n), "{n}");
        }
    }

    #[test]
    fn real_adapters_are_kept() {
        for n in ["Wi-Fi", "Ethernet", "이더넷", "Realtek PCIe GbE Family Controller"] {
            assert!(!is_virtual(n), "{n}");
        }
    }
}
