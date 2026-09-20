//! 重要性分类:决定一个进程是否**允许**被削减。
//! 这是本工具最重要的安全边界 —— 前台、系统会话与白名单一律不可触碰。

use crate::metrics::{ProcessInfo, Snapshot};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Importance {
    /// 前台窗口进程及其进程树:用户正在交互,永不触碰。
    Foreground,
    /// Session 0:系统/服务进程,永不触碰。
    System,
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

/// 分类优先级:前台 > 系统 > 白名单 > 后台。
pub fn classify(p: &ProcessInfo, s: &Snapshot) -> Importance {
    if s.foreground_pids.contains(&p.pid) {
        return Importance::Foreground;
    }
    if p.session_id == 0 {
        return Importance::System;
    }
    let name = p.name.to_lowercase();
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
    fn whitelist_is_case_insensitive() {
        let s = snap(&["NODE.EXE"], &[]);
        assert_eq!(classify(&proc(100, "node.exe", 1), &s), Importance::Whitelisted);
    }

    #[test]
    fn ordinary_process_is_background() {
        let s = snap(&[], &[]);
        assert_eq!(classify(&proc(100, "node.exe", 1), &s), Importance::Background);
    }

    #[test]
    fn only_background_is_eligible_for_action() {
        assert!(Importance::Background.is_eligible());
        assert!(!Importance::Foreground.is_eligible());
        assert!(!Importance::System.is_eligible());
        assert!(!Importance::Whitelisted.is_eligible());
    }
}
