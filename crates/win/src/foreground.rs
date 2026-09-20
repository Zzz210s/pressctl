//! 前台窗口进程**及其全部后代**。这些进程永不触碰 —— 用户正在与它们交互。
//! 后代保护是必需的:终端窗口本身是前台,但用户实际在用的 shell 是它的子进程。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// 前台窗口进程自身 + 全部后代(去重升序)。
pub fn foreground_pids() -> Vec<u32> {
    match foreground_window_pid() {
        Some(pid) => crate::tree::tree_of(pid),
        None => Vec::new(),
    }
}

/// 仅前台窗口所属进程的 pid。
pub fn foreground_window_pid() -> Option<u32> {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            None
        } else {
            Some(pid)
        }
    }
}
