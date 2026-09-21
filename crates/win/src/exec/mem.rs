//! 内存类动作:修剪工作集与清理待机列表。两者都不终止任何进程。

use pressctl_core::metrics::ProcessInfo;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
};

/// 修剪单个进程的工作集:把页刷到页面文件,降低其驻留内存。
pub fn trim_working_set(pid: u32) -> Result<(), String> {
    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA,
            false,
            pid,
        )
        .map_err(|e| format!("OpenProcess(pid {pid}) 失败: {e}"))?;
        let res = EmptyWorkingSet(handle);
        let _ = CloseHandle(handle);
        res.map_err(|e| format!("EmptyWorkingSet(pid {pid}) 失败: {e}"))
    }
}

/// 按工作集降序修剪一批进程,返回成功条数与首个错误。
pub fn trim_batch(procs: &[ProcessInfo]) -> (usize, Option<String>) {
    let mut ok = 0usize;
    let mut first_err = None;
    for p in procs {
        match trim_working_set(p.pid) {
            Ok(()) => ok += 1,
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
    }
    (ok, first_err)
}

/// 清理待机(缓存)列表。
///
/// 使用未文档化的 `NtSetSystemInformation(SystemMemoryListInformation, MemoryPurgeStandbyList)`。
/// 该调用需要 `SeProfileSingleProcessPrivilege`;权限不足时返回错误而不会造成任何副作用。
pub fn purge_standby() -> Result<(), String> {
    const SYSTEM_MEMORY_LIST_INFORMATION: u32 = 80;
    const MEMORY_PURGE_STANDBY_LIST: i32 = 4;

    #[link(name = "ntdll")]
    extern "system" {
        fn NtSetSystemInformation(
            system_information_class: u32,
            system_information: *mut core::ffi::c_void,
            system_information_length: u32,
        ) -> i32;
    }

    let mut command = MEMORY_PURGE_STANDBY_LIST;
    let status = unsafe {
        NtSetSystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut command as *mut i32 as *mut core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };
    if status >= 0 {
        Ok(())
    } else {
        Err(format!(
            "NtSetSystemInformation 返回 0x{:08X}(通常为权限不足)",
            status as u32
        ))
    }
}
