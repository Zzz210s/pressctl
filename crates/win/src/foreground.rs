//! 前台窗口进程。这些进程永不触碰 —— 用户正在与它们交互。
//! 本阶段只保护前台进程本身;完整进程树在 MVP-2 补齐。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

pub fn foreground_pids() -> Vec<u32> {
    let mut pids = Vec::new();
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.0.is_null() {
            return pids;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != 0 {
            pids.push(pid);
        }
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}
