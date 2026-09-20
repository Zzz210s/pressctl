//! Windows 采集层:只负责把系统状态读成 `pressctl_core` 定义的结构体。
//! 本层不做任何判断,决策全部在 `pressctl_core` 中完成。

pub use pressctl_core as core;

pub mod cpu;
pub mod foreground;
pub mod memory;
pub mod processes;
pub mod tree;

use pressctl_core::metrics::{PolicyConfig, Snapshot};

/// CPU 采样窗口(毫秒)。CPU 是速率,必须两次采样求差。
pub const CPU_WINDOW_MS: u64 = 300;

/// 采集一次完整快照。整机与每进程 CPU 占用由 `cpu::sample_cpu` 填充,
/// 前台豁免集合包含前台窗口进程及其全部后代。
pub fn snapshot(config: PolicyConfig) -> Snapshot {
    let sample = cpu::sample_cpu(CPU_WINDOW_MS);

    let mut procs = processes::read_processes();
    for p in &mut procs {
        p.cpu_percent = sample.per_process.get(&p.pid).copied().unwrap_or(0.0);
    }

    Snapshot {
        memory: memory::read_memory(),
        processes: procs,
        foreground_pids: foreground::foreground_pids(),
        frozen_pids: Vec::new(),
        cpu_used_percent: sample.system_percent,
        config,
    }
}
