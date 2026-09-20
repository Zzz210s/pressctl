//! 比例化 CPU 节流:把压力换算成"保留原有速度的百分比"。
//!
//! 分段的理由:线性映射会在压力刚过阈值时过度压制,也会在接近临界时压制不足。
//! 分段 + 下限的映射既能快速响应,又保证任何进程都不会被压到无法推进。

use crate::metrics::{PolicyConfig, ProcessInfo};

/// 返回 0.0..=1.0 的保留比率(1.0 = 不节流)。
pub fn throttle_ratio(used_percent: f32, cfg: &PolicyConfig) -> f32 {
    let raw = if used_percent >= cfg.critical_percent {
        cfg.cpu_throttle_floor
    } else if used_percent >= cfg.high_percent {
        0.50
    } else if used_percent >= cfg.warn_percent {
        0.75
    } else {
        1.0
    };
    raw.max(cfg.cpu_throttle_floor).min(1.0)
}

/// 生成节流计划:只包含 CPU 占用大于 0 的进程,按 CPU 降序。
/// 返回 `(进程引用, 保留比率)`。
pub fn throttle_plan(procs: &[ProcessInfo], ratio: f32) -> Vec<(&ProcessInfo, f32)> {
    let mut v: Vec<(&ProcessInfo, f32)> = procs
        .iter()
        .filter(|p| p.cpu_percent > 0.0)
        .map(|p| (p, ratio))
        .collect();
    v.sort_by(|a, b| {
        b.0.cpu_percent
            .partial_cmp(&a.0.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(floor: f32) -> PolicyConfig {
        PolicyConfig {
            warn_percent: 85.0,
            high_percent: 92.0,
            critical_percent: 97.0,
            cpu_high_percent: 90.0,
            min_working_set_bytes: 0,
            cpu_throttle_floor: floor,
            kill_enabled: false,
            whitelist: Vec::new(),
        }
    }

    fn proc(pid: u32, cpu: f32) -> ProcessInfo {
        ProcessInfo {
            pid,
            name: format!("p{pid}.exe"),
            working_set_bytes: 1000,
            private_bytes: 900,
            cpu_percent: cpu,
            session_id: 1,
        }
    }

    #[test]
    fn idle_pressure_means_no_throttle() {
        assert_eq!(throttle_ratio(50.0, &cfg(0.25)), 1.0);
    }

    #[test]
    fn warn_pressure_throttles_mildly() {
        assert_eq!(throttle_ratio(85.0, &cfg(0.25)), 0.75);
    }

    #[test]
    fn high_pressure_throttles_harder() {
        assert_eq!(throttle_ratio(92.0, &cfg(0.25)), 0.50);
    }

    #[test]
    fn critical_pressure_hits_the_floor() {
        assert_eq!(throttle_ratio(97.0, &cfg(0.25)), 0.25);
    }

    #[test]
    fn ratio_never_goes_below_floor() {
        assert_eq!(throttle_ratio(99.9, &cfg(0.60)), 0.60);
    }

    #[test]
    fn plan_is_sorted_by_cpu_desc() {
        let procs = vec![proc(1, 5.0), proc(2, 50.0), proc(3, 20.0)];
        let plan = throttle_plan(&procs, 0.5);
        let pids: Vec<u32> = plan.iter().map(|(p, _)| p.pid).collect();
        assert_eq!(pids, vec![2, 3, 1]);
    }

    #[test]
    fn plan_skips_zero_cpu_processes() {
        let procs = vec![proc(1, 0.0), proc(2, 20.0)];
        let plan = throttle_plan(&procs, 0.5);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].0.pid, 2);
    }
}
