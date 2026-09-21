//! 控制台中断处理:让 Ctrl+C 变成"可优雅收尾"的信号,而不是直接被杀。
//!
//! 为什么需要:被挂起的进程不会因为本工具退出而自动恢复,所以必须在退出前回滚。

use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::{BOOL, TRUE};
use windows::Win32::System::Console::SetConsoleCtrlHandler;

static STOP: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn handler(_ctrl_type: u32) -> BOOL {
    STOP.store(true, Ordering::SeqCst);
    // 返回 TRUE:抑制默认的立即终止行为,让主循环有机会回滚。
    TRUE
}

/// 安装处理器。安装后 `stop_requested()` 会在 Ctrl+C / 关闭窗口时变为 true。
pub fn install_stop_handler() -> Result<(), String> {
    unsafe {
        SetConsoleCtrlHandler(Some(handler), true)
            .map_err(|e| format!("SetConsoleCtrlHandler 失败: {e}"))
    }
}

/// 是否已请求停止(用户按了 Ctrl+C)。
pub fn stop_requested() -> bool {
    STOP.load(Ordering::SeqCst)
}
