use super::*;
use crate::metrics::{MemoryMetrics, PolicyConfig};

fn snap(used_pct: f32, procs: Vec<ProcessInfo>, kill: bool) -> Snapshot {
    let total = 100u64;
    let available = (100.0 - used_pct) as u64;
    Snapshot {
        memory: MemoryMetrics {
            total_bytes: total,
            available_bytes: available,
            commit_total_bytes: total,
            commit_limit_bytes: total * 2,
            standby_bytes: 1024,
        },
        processes: procs,
        foreground_pids: Vec::new(),
        cpu_used_percent: 10.0,
        config: PolicyConfig {
            warn_percent: 85.0,
            high_percent: 92.0,
            critical_percent: 97.0,
            cpu_high_percent: 90.0,
            min_working_set_bytes: 500,
            cpu_throttle_floor: 0.25,
            kill_enabled: kill,
            whitelist: Vec::new(),
        },
    }
}

fn proc(pid: u32, name: &str, ws: u64, cpu: f32) -> ProcessInfo {
    ProcessInfo {
        pid,
        name: name.into(),
        working_set_bytes: ws,
        private_bytes: ws,
        cpu_percent: cpu,
        session_id: 1,
    }
}

#[test]
fn idle_produces_no_actions() {
    let d = decide(&snap(50.0, vec![proc(1, "a.exe", 9999, 50.0)], false));
    assert_eq!(d.level, PressureLevel::Idle);
    assert!(d.actions.is_empty());
}

#[test]
fn warn_purges_standby_and_throttles() {
    let d = decide(&snap(86.0, vec![proc(1, "a.exe", 9999, 50.0)], false));
    assert!(matches!(d.actions[0], Action::PurgeStandby { .. }));
    assert!(d
        .actions
        .iter()
        .any(|a| matches!(a, Action::ThrottleCpu { ratio, .. } if (*ratio - 0.75).abs() < 1e-6)));
}

#[test]
fn high_also_trims_big_working_sets() {
    let d = decide(&snap(93.0, vec![proc(1, "a.exe", 9999, 50.0)], false));
    assert!(d
        .actions
        .iter()
        .any(|a| matches!(a, Action::TrimWorkingSet { .. })));
}

#[test]
fn trim_skips_processes_below_min_working_set() {
    let d = decide(&snap(93.0, vec![proc(1, "a.exe", 100, 50.0)], false));
    assert!(!d
        .actions
        .iter()
        .any(|a| matches!(a, Action::TrimWorkingSet { .. })));
}

#[test]
fn foreground_process_is_never_acted_on() {
    let mut s = snap(98.0, vec![proc(1, "a.exe", 9999, 99.0)], true);
    s.foreground_pids = vec![1];
    let d = decide(&s);
    assert!(!d.actions.iter().any(|a| matches!(
        a,
        Action::TrimWorkingSet { .. } | Action::ThrottleCpu { .. } | Action::KillProcess { .. }
    )));
}

#[test]
fn critical_with_kill_enabled_picks_exactly_one_victim() {
    let d = decide(&snap(
        98.0,
        vec![proc(1, "a.exe", 9999, 50.0), proc(2, "b.exe", 100, 1.0)],
        true,
    ));
    let kills = d
        .actions
        .iter()
        .filter(|a| matches!(a, Action::KillProcess { .. }))
        .count();
    assert_eq!(kills, 1);
}

#[test]
fn critical_without_kill_enabled_has_no_kill() {
    let d = decide(&snap(98.0, vec![proc(1, "a.exe", 9999, 50.0)], false));
    assert!(!d
        .actions
        .iter()
        .any(|a| matches!(a, Action::KillProcess { .. })));
}

#[test]
fn kill_never_targets_foreground() {
    let mut s = snap(
        98.0,
        vec![proc(1, "a.exe", 9999, 99.0), proc(2, "b.exe", 8000, 40.0)],
        true,
    );
    s.foreground_pids = vec![1];
    let d = decide(&s);
    assert!(d
        .actions
        .iter()
        .any(|a| matches!(a, Action::KillProcess { pid, .. } if *pid == 2)));
}
