//! CPU 采样:CPU 占用是"速率",必须两次采样求差,无法单点读取。
//! 百分比按逻辑核数归一,即 100% = 占满整机全部核心。

use std::collections::HashMap;
use windows::Win32::Foundation::{CloseHandle, FILETIME};
use windows::Win32::System::Threading::{
    GetProcessTimes, GetSystemTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// 一次采样的结果。
pub struct CpuSample {
    /// 整机 CPU 占用百分比(0.0..=100.0)。
    pub system_percent: f32,
    /// 每进程 CPU 占用百分比(0.0..=100.0,已按核数归一)。
    pub per_process: HashMap<u32, f32>,
}

fn to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

fn system_times() -> Option<(u64, u64, u64)> {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()?;
    }
    Some((to_u64(idle), to_u64(kernel), to_u64(user)))
}

fn process_times(pid: u32) -> Option<u64> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let ok = GetProcessTimes(
            handle,
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return None;
        }
        Some(to_u64(kernel) + to_u64(user))
    }
}

fn snapshot_times() -> HashMap<u32, u64> {
    let mut m = HashMap::new();
    for p in crate::processes::read_processes() {
        if let Some(t) = process_times(p.pid) {
            m.insert(p.pid, t);
        }
    }
    m
}

/// 在 `window_ms` 毫秒窗口内采样,返回整机与每进程的 CPU 占用。
pub fn sample_cpu(window_ms: u64) -> CpuSample {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get() as f64)
        .unwrap_or(1.0);

    let sys_a = system_times();
    let proc_a = snapshot_times();
    std::thread::sleep(std::time::Duration::from_millis(window_ms));
    let sys_b = system_times();
    let proc_b = snapshot_times();

    let system_percent = match (sys_a, sys_b) {
        (Some((idle_a, k_a, u_a)), Some((idle_b, k_b, u_b))) => {
            let idle = idle_b.saturating_sub(idle_a) as f64;
            // 注意:Windows 的 kernel time 已包含 idle time
            let total = (k_b.saturating_sub(k_a) + u_b.saturating_sub(u_a)) as f64;
            if total <= 0.0 {
                0.0
            } else {
                (((total - idle) / total) * 100.0).clamp(0.0, 100.0) as f32
            }
        }
        _ => 0.0,
    };

    // FILETIME 单位是 100 纳秒;窗口总时长换算到同一单位。
    let window_100ns = (window_ms as f64) * 10_000.0;
    let mut per_process = HashMap::new();
    for (pid, t_b) in proc_b {
        if let Some(t_a) = proc_a.get(&pid) {
            let delta = t_b.saturating_sub(*t_a) as f64;
            let pct = delta / (window_100ns * cores) * 100.0;
            per_process.insert(pid, pct.clamp(0.0, 100.0) as f32);
        }
    }

    CpuSample {
        system_percent,
        per_process,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_returns_plausible_values() {
        let s = sample_cpu(120);
        assert!((0.0..=100.0).contains(&s.system_percent), "system {}", s.system_percent);
        for (pid, pct) in &s.per_process {
            assert!((0.0..=100.0).contains(pct), "pid {pid} pct {pct}");
        }
    }

    #[test]
    fn sample_finds_at_least_this_process() {
        let me = std::process::id();
        let s = sample_cpu(120);
        assert!(s.per_process.contains_key(&me), "own pid missing from sample");
    }
}
