//! 物理内存采集。待机列表大小需要未文档化的 `NtQuerySystemInformation`,
//! 本阶段先返回 0,以保证其余指标永远可用(见计划 MVP-2)。

use pressctl_core::metrics::MemoryMetrics;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

pub fn read_memory() -> MemoryMetrics {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    let mut total = 0u64;
    let mut avail = 0u64;
    let mut commit_total = 0u64;
    let mut commit_limit = 0u64;
    unsafe {
        if GlobalMemoryStatusEx(&mut status).is_ok() {
            total = status.ullTotalPhys;
            avail = status.ullAvailPhys;
            commit_limit = status.ullTotalPageFile;
            commit_total = status
                .ullTotalPageFile
                .saturating_sub(status.ullAvailPageFile);
        }
    }
    MemoryMetrics {
        total_bytes: total,
        available_bytes: avail,
        commit_total_bytes: commit_total,
        commit_limit_bytes: commit_limit,
        standby_bytes: 0,
    }
}
