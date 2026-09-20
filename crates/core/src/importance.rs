//! 重要性分类:决定一个进程是否**允许**被削减。
//! 这是本工具最重要的安全边界 —— 前台、系统会话、交互宿主与白名单一律不可触碰。

use crate::metrics::{ProcessInfo, Snapshot};
use serde::{Deserialize, Serialize};

/// 内置的"交互宿主"名单。
///
/// 为什么需要它:Windows 不会在父进程退出后重挂父链(与 Unix 不同),因此
/// 前台窗口 -> shell 的链条可能断裂(实测:pi 会话的 `node <- sh.exe <- 已退出`)。
/// 只靠前台进程树会漏掉这类孤立 shell。而这些进程一旦被节流/冻结,用户的直接
/// 操作会明显变卡,且它们自身占用极小 —— 所以永久豁免。
pub const INTERACTIVE_HOSTS: &[&str] = &[
    "explorer.exe",
    "cmd.exe",
    "conhost.exe",
    "openconsole.exe",
    "windowsterminal.exe",
    "wt.exe",
    "bash.exe",
    "sh.exe",
    "wsl.exe",
    "wslhost.exe",
    "powershell.exe",
    "pwsh.exe",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Importance {
    /// 前台窗口进程及其进程树:用户正在交互,永不触碰。
    Foreground,
    /// Session 0:系统/服务进程,永不触碰。
    System,
    /// 交互宿主(shell / 终端 / 桌面):节流它会直接拖慢用户操作,永不触碰。
    InteractiveHost,
    /// 用户白名单中的可执行文件名,永不触碰。
    Whitelisted,
    /// 普通后台用户进程:唯一可被削减的对象。
    Background,
}

impl Importance {
    /// 是否允许对其执行削减动作。
    pub fn is_eligible(self) -> bool {
        matches!(self, Importance::Background)
    }
}

/// 分类优先级:前台 > 系统 > 交互宿主 > 白名单 > 后台。
pub fn classify(p: &ProcessInfo, s: &Snapshot) -> Importance {
    if s.foreground_pids.contains(&p.pid) {
        return Importance::Foreground;
    }
    if p.session_id == 0 {
        return Importance::System;
    }
    let name = p.name.to_lowercase();
    if INTERACTIVE_HOSTS.iter().any(|h| *h == name) {
        return Importance::InteractiveHost;
    }
    if s.config.whitelist.iter().any(|w| w.to_lowercase() == name) {
        return Importance::Whitelisted;
    }
    Importance::Background
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{MemoryMetrics, PolicyConfig};

    fn snap(whitelist: &[&str], foreground: &[u32]) -> Snapshot {
        Snapshot {
            memory: MemoryMetrics {
                total_bytes: 100,
                available_bytes: 50,
                commit_total_bytes: 100,
                commit_limit_bytes: 200,
                standby_bytes: 0,
            },
            processes: Vec::new(),
            foreground_pids: foreground.to_vec(),
            cpu_used_percent: 10.0,
            config: PolicyConfig {
                warn_percent: 85.0,
                high_percent: 92.0,
                critical_percent: 97.0,
                cpu_high_percent: 90.0,
                min_working_set_bytes: 0,
                cpu_throttle_floor: 0.25,
                kill_enabled: false,
                whitelist: whitelist.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    fn proc(pid: u32, name: &str, session: u32) -> ProcessInfo {
        ProcessInfo {
            pid,
            name: name.into(),
            working_set_bytes: 1000,
            private_bytes: 900,
            cpu_percent: 10.0,
            session_id: session,
        }
    }

    #[test]
    fn foreground_wins_over_everything() {
        let s = snap(&["node.exe"], &[100]);
        assert_eq!(classify(&proc(100, "node.exe", 1), &s), Importance::Foreground);
    }

    #[test]
    fn session_zero_is_system() {
        let s = snap(&[], &[]);
        assert_eq!(classify(&proc(4, "System", 0), &s), Importance::System);
    }

    #[test]
    fn shell_is_interactive_host_even_when_orphaned() {
        let s = snap(&[], &[]);
        assert_eq!(classify(&proc(1348, "sh.exe", 1), &s), Importance::InteractiveHost);
        assert_eq!(classify(&proc(999, "bash.exe", 1), &s), Importance::InteractiveHost);
        assert_eq!(
            classify(&proc(998, "WindowsTerminal.exe", 1), &s),
            Importance::InteractiveHost
        );
    }

    #[test]
    fn interactive_host_is_case_insensitive() {
        let s = snap(&[], &[]);
        assert_eq!(classify(&proc(1, "PWSH.EXE", 1), &s), Importance::InteractiveHost);
    }

    #[test]
    fn heavy_runtime_is_still_throttlable() {
        // node.exe 是"要削减的大户",不能被内置豁免挡住
        let s = snap(&[], &[]);
        assert_eq!(classify(&proc(100, "node.exe", 1), &s), Importance::Background);
    }

    #[test]
    fn whitelist_is_case_insensitive() {
        let s = snap(&["NODE.EXE"], &[]);
        assert_eq!(classify(&proc(100, "node.exe", 1), &s), Importance::Whitelisted);
    }

    #[test]
    fn ordinary_process_is_background() {
        let s = snap(&[], &[]);
        assert_eq!(classify(&proc(100, "someapp.exe", 1), &s), Importance::Background);
    }

    #[test]
    fn only_background_is_eligible_for_action() {
        assert!(Importance::Background.is_eligible());
        assert!(!Importance::Foreground.is_eligible());
        assert!(!Importance::System.is_eligible());
        assert!(!Importance::InteractiveHost.is_eligible());
        assert!(!Importance::Whitelisted.is_eligible());
    }
}
