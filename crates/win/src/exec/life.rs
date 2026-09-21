//! 生命周期类动作:挂起(冻结)、恢复、终止。
//!
//! 冻结用 `SuspendThread` 逐个挂起线程,而不是未文档化的 `NtSuspendProcess` ——
//! 前者是文档化 API。已知取舍:挂起之后新创建的线程不会被挂起。

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows::Win32::System::Threading::{
    OpenThread, ResumeThread, SuspendThread, TerminateProcess,
    THREAD_SUSPEND_RESUME,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE};

/// 打开进程句柄并交给 `f`,确保句柄被关闭。
fn with_process<T>(
    pid: u32,
    access: windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS,
    f: impl FnOnce(HANDLE) -> T,
) -> Result<T, String> {
    unsafe {
        let h = OpenProcess(access, false, pid).map_err(|e| format!("OpenProcess(pid {pid}): {e}"))?;
        let out = f(h);
        let _ = CloseHandle(h);
        Ok(out)
    }
}

/// 枚举某进程的全部线程 id。
fn threads_of(pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) {
            Ok(h) => h,
            Err(_) => return out,
        };
        let mut te = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        if Thread32First(snap, &mut te).is_ok() {
            loop {
                if te.th32OwnerProcessID == pid {
                    out.push(te.th32ThreadID);
                }
                if Thread32Next(snap, &mut te).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// 对某进程的每个线程施加 `op`,返回成功次数。
fn for_each_thread(pid: u32, op: fn(HANDLE) -> u32) -> usize {
    let access = THREAD_SUSPEND_RESUME;
    let mut ok = 0usize;
    for tid in threads_of(pid) {
        unsafe {
            if let Ok(h) = OpenThread(access, false, tid) {
                if op(h) != u32::MAX {
                    ok += 1;
                }
                let _ = CloseHandle(h);
            }
        }
    }
    ok
}

/// 挂起进程的全部线程,返回被挂起的线程数。
pub fn suspend(pid: u32) -> Result<usize, String> {
    let n = for_each_thread(pid, |h| unsafe { SuspendThread(h) });
    if n == 0 {
        Err(format!("未挂起任何线程(pid {pid})"))
    } else {
        Ok(n)
    }
}

/// 恢复进程的全部线程,返回被恢复的线程数。
pub fn resume(pid: u32) -> Result<usize, String> {
    let n = for_each_thread(pid, |h| unsafe { ResumeThread(h) });
    if n == 0 {
        Err(format!("未恢复任何线程(pid {pid})"))
    } else {
        Ok(n)
    }
}

/// 终止进程。这是最后手段,不可回滚。
pub fn terminate(pid: u32) -> Result<(), String> {
    with_process(pid, PROCESS_TERMINATE, |h| unsafe {
        TerminateProcess(h, 1).map_err(|e| format!("TerminateProcess(pid {pid}): {e}"))
    })?
}
