//! 人读报告。全部输出为 ASCII/中文,不含 emoji。

use crate::decision::{Action, Decision};
use crate::metrics::Snapshot;

fn mb(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / 1024.0 / 1024.0)
}

fn describe(a: &Action) -> String {
    match a {
        Action::PurgeStandby { bytes } => {
            format!("purge standby list (可回收约 {})", mb(*bytes))
        }
        Action::TrimWorkingSet { pid, name, bytes } => {
            format!("trim working set: {name} (pid {pid}, {})", mb(*bytes))
        }
        Action::ThrottleCpu { pid, name, ratio } => {
            format!("throttle cpu: {name} (pid {pid}) -> keep {ratio:.2}")
        }
        Action::KillProcess { pid, name, reason } => {
            format!("kill process (LAST RESORT): {name} (pid {pid}) - {reason}")
        }
    }
}

pub fn render_human(d: &Decision, s: &Snapshot) -> String {
    let mut out = String::new();
    out.push_str("=== pressctl (DRY-RUN, nothing is executed) ===\n");
    out.push_str(&format!(
        "memory: used {:.1}%  ({} free of {})\n",
        d.memory_used_percent,
        mb(s.memory.available_bytes),
        mb(s.memory.total_bytes)
    ));
    out.push_str(&format!("cpu:    used {:.1}%\n", d.cpu_used_percent));
    out.push_str(&format!("level:  {:?}\n", d.level));
    out.push_str(&format!("actions: {}\n", d.actions.len()));
    for a in &d.actions {
        out.push_str(&format!("  - {}\n", describe(a)));
    }
    if !d.notes.is_empty() {
        out.push_str("notes:\n");
        for n in &d.notes {
            out.push_str(&format!("  - {n}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{MemoryMetrics, PolicyConfig};
    use crate::pressure::PressureLevel;

    fn snap() -> Snapshot {
        Snapshot {
            memory: MemoryMetrics {
                total_bytes: 16 * 1024 * 1024 * 1024,
                available_bytes: 1024 * 1024 * 1024,
                commit_total_bytes: 20 * 1024 * 1024 * 1024,
                commit_limit_bytes: 32 * 1024 * 1024 * 1024,
                standby_bytes: 800 * 1024 * 1024,
            },
            processes: Vec::new(),
            foreground_pids: Vec::new(),
            cpu_used_percent: 42.5,
            config: PolicyConfig {
                warn_percent: 85.0,
                high_percent: 92.0,
                critical_percent: 97.0,
                cpu_high_percent: 90.0,
                min_working_set_bytes: 0,
                cpu_throttle_floor: 0.25,
                kill_enabled: false,
                whitelist: Vec::new(),
            },
        }
    }

    fn decision(level: PressureLevel, actions: Vec<Action>, notes: Vec<String>) -> Decision {
        Decision {
            level,
            memory_used_percent: 93.8,
            cpu_used_percent: 42.5,
            actions,
            notes,
        }
    }

    #[test]
    fn header_reports_percent_and_level() {
        let d = decision(PressureLevel::High, Vec::new(), vec!["test note".into()]);
        let out = render_human(&d, &snap());
        assert!(out.contains("93.8%"));
        assert!(out.contains("High"));
        assert!(out.contains("test note"));
    }

    #[test]
    fn dry_run_banner_is_present() {
        let d = decision(PressureLevel::Idle, Vec::new(), Vec::new());
        assert!(render_human(&d, &snap()).contains("DRY-RUN"));
    }

    #[test]
    fn actions_are_listed_with_pid() {
        let d = decision(
            PressureLevel::Warn,
            vec![Action::ThrottleCpu {
                pid: 1234,
                name: "node.exe".into(),
                ratio: 0.75,
            }],
            Vec::new(),
        );
        let out = render_human(&d, &snap());
        assert!(out.contains("node.exe"));
        assert!(out.contains("1234"));
        assert!(out.contains("0.75"));
    }

    #[test]
    fn kill_action_is_marked_as_last_resort() {
        let d = decision(
            PressureLevel::Critical,
            vec![Action::KillProcess {
                pid: 9,
                name: "x.exe".into(),
                reason: "r".into(),
            }],
            Vec::new(),
        );
        assert!(render_human(&d, &snap()).contains("LAST RESORT"));
    }

    #[test]
    fn no_emoji_in_output() {
        let d = decision(
            PressureLevel::Critical,
            vec![Action::KillProcess {
                pid: 9,
                name: "x.exe".into(),
                reason: "r".into(),
            }],
            vec!["n".into()],
        );
        let out = render_human(&d, &snap());
        assert!(!out.chars().any(|c| c as u32 > 0xFFFF));
    }
}
