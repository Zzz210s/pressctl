//! 进程枚举:pid、可执行文件名、工作集、私有内存、会话 ID。
//! CPU 百分比需要两次采样求差,由 CLI 层负责,本层只填 0.0。

use pressctl_core::metrics::ProcessInfo;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::ProcessStatus::{
    EnumProcesses, GetProcessImageFileNameW, GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};

const MAX_PID_COUNT: usize = 4096;
const NAME_BUF_LEN: usize = 260;

pub fn read_processes() -> Vec<ProcessInfo> {
    let mut pids = vec![0u32; MAX_PID_COUNT];
    let mut needed = 0u32;
    unsafe {
        if EnumProcesses(
            pids.as_mut_ptr(),
            (pids.len() * std::mem::size_of::<u32>()) as u32,
            &mut needed,
        )
        .is_err()
        {
            return Vec::new();
        }
    }
    let count = (needed as usize / std::mem::size_of::<u32>()).min(MAX_PID_COUNT);
    pids.truncate(count);

    let mut out = Vec::with_capacity(count);
    for pid in pids {
        if pid == 0 {
            continue;
        }
        if let Some(p) = read_one(pid) {
            out.push(p);
        }
    }
    out
}

fn read_one(pid: u32) -> Option<ProcessInfo> {
    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            false,
            pid,
        )
        .ok()?;

        let mut buf = vec![0u16; NAME_BUF_LEN];
        let len = GetProcessImageFileNameW(handle, &mut buf);
        let full = if len > 0 {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            String::new()
        };
        let name = full.rsplit('\\').next().unwrap_or(&full).to_string();

        let mut counters = PROCESS_MEMORY_COUNTERS::default();
        let ok = GetProcessMemoryInfo(
            handle,
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
        .is_ok();

        let mut session = 0u32;
        let _ = ProcessIdToSessionId(pid, &mut session);
        let _ = CloseHandle(handle);

        if name.is_empty() {
            return None;
        }
        Some(ProcessInfo {
            pid,
            name,
            working_set_bytes: if ok { counters.WorkingSetSize as u64 } else { 0 },
            private_bytes: if ok { counters.PagefileUsage as u64 } else { 0 },
            cpu_percent: 0.0,
            session_id: session,
        })
    }
}
