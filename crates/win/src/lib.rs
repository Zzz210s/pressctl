//! Windows 采集层:只负责把系统状态读成 `pressctl_core` 定义的结构体。
//! 本层不做任何判断,决策全部在 `pressctl_core` 中完成。

pub use pressctl_core as core;

pub mod foreground;
pub mod memory;
pub mod processes;

use pressctl_core::metrics::{PolicyConfig, Snapshot};

/// 采集一次完整快照。CPU 使用率由 CLI 层通过两次采样补齐。
pub fn snapshot(config: PolicyConfig) -> Snapshot {
    Snapshot {
        memory: memory::read_memory(),
        processes: processes::read_processes(),
        foreground_pids: foreground::foreground_pids(),
        cpu_used_percent: 0.0,
        config,
    }
}
