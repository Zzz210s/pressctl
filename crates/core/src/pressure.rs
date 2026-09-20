//! 内存压力分级:把可用内存换算成等级,作为决策阶梯的输入。

use crate::metrics::{MemoryMetrics, PolicyConfig};
use serde::{Deserialize, Serialize};

/// 压力等级。级别越高,允许的动作越激进。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PressureLevel {
    Idle,
    Warn,
    High,
    Critical,
}

/// 已用内存百分比,保留一位小数。
pub fn memory_used_percent(m: &MemoryMetrics) -> f32 {
    if m.total_bytes == 0 {
        return 0.0;
    }
    let used = m.total_bytes.saturating_sub(m.available_bytes);
    let pct = used as f64 / m.total_bytes as f64 * 100.0;
    (pct * 10.0).round() / 10.0
}

/// 依据阈值把已用内存百分比映射为压力等级(含等号:达到阈值即进入该级)。
pub fn pressure_level(m: &MemoryMetrics, cfg: &PolicyConfig) -> PressureLevel {
    let pct = memory_used_percent(m);
    if pct >= cfg.critical_percent {
        PressureLevel::Critical
    } else if pct >= cfg.high_percent {
        PressureLevel::High
    } else if pct >= cfg.warn_percent {
        PressureLevel::Warn
    } else {
        PressureLevel::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(total: u64, available: u64) -> MemoryMetrics {
        MemoryMetrics {
            total_bytes: total,
            available_bytes: available,
            commit_total_bytes: total,
            commit_limit_bytes: total * 2,
            standby_bytes: 0,
        }
    }

    fn cfg() -> PolicyConfig {
        PolicyConfig {
            warn_percent: 85.0,
            high_percent: 92.0,
            critical_percent: 97.0,
            cpu_high_percent: 90.0,
            min_working_set_bytes: 200 * 1024 * 1024,
            cpu_throttle_floor: 0.25,
            kill_enabled: false,
            whitelist: Vec::new(),
        }
    }

    #[test]
    fn used_percent_is_rounded_to_one_decimal() {
        // 16 GiB 里只剩 3.2 GiB 可用 -> 80.0%
        let m = mem(16 * 1024 * 1024 * 1024, 32 * 1024 * 1024 * 1024 / 10);
        assert_eq!(memory_used_percent(&m), 80.0);
    }

    #[test]
    fn used_percent_handles_zero_total() {
        assert_eq!(memory_used_percent(&mem(0, 0)), 0.0);
    }

    #[test]
    fn level_is_idle_below_warn() {
        let m = mem(100, 20); // 80%
        assert_eq!(pressure_level(&m, &cfg()), PressureLevel::Idle);
    }

    #[test]
    fn level_is_warn_at_warn_threshold() {
        let m = mem(100, 15); // 85%
        assert_eq!(pressure_level(&m, &cfg()), PressureLevel::Warn);
    }

    #[test]
    fn level_is_high_at_high_threshold() {
        let m = mem(100, 8); // 92%
        assert_eq!(pressure_level(&m, &cfg()), PressureLevel::High);
    }

    #[test]
    fn level_is_critical_at_critical_threshold() {
        let m = mem(100, 3); // 97%
        assert_eq!(pressure_level(&m, &cfg()), PressureLevel::Critical);
    }
}
