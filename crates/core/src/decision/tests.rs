use super::*;
use crate::metrics::{MemoryMetrics, PolicyConfig};

fn snap_with(used_pct: f32, procs: Vec<ProcessInfo>, kill: bool, frozen: Vec<u32>) -> Snapshot {
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
        frozen_pids: frozen,
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

fn snap(used_pct: f32, procs: Vec<ProcessInfo>, kill: bool) -> Snapshot {
    snap_with(used_pct, procs, kill, Vec::new())
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

fn count_kills(d: &Decision) -> usize {
    d.actions
        .iter()
        .filter(|a| matches!(a, Action::KillProcess { .. }))
        .count()
}

#[test]
fn idle_produces_no_reduction_actions() {
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
        Action::TrimWorkingSet { .. }
            | Action::ThrottleCpu { .. }
            | Action::FreezeProcess { .. }
            | Action::KillProcess { .. }
    )));
}

#[test]
fn critical_freezes_instead_of_killing() {
    let d = decide(&snap(98.0, vec![proc(1, "a.exe", 9999, 50.0)], true));
    assert!(d
        .actions
        .iter()
        .any(|a| matches!(a, Action::FreezeProcess { .. })));
    assert_eq!(count_kills(&d), 0, "未先尝试冻结就不应终止");
}

#[test]
fn freeze_picks_the_worst_offender_only() {
    let d = decide(&snap(
        98.0,
        vec![proc(1, "a.exe", 9999, 50.0), proc(2, "b.exe", 100, 1.0)],
        false,
    ));
    let frozen: Vec<u32> = d
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::FreezeProcess { pid, .. } => Some(*pid),
            _ => None,
        })
        .collect();
    assert_eq!(frozen, vec![1]);
}

#[test]
fn critical_with_existing_freeze_and_kill_enabled_escalates() {
    let d = decide(&snap_with(
        98.0,
        vec![proc(1, "a.exe", 9999, 50.0), proc(2, "b.exe", 100, 1.0)],
        true,
        vec![2],
    ));
    assert_eq!(count_kills(&d), 1);
    assert!(!d
        .actions
        .iter()
        .any(|a| matches!(a, Action::FreezeProcess { .. })));
}

#[test]
fn critical_with_existing_freeze_but_kill_disabled_does_not_kill() {
    let d = decide(&snap_with(
        98.0,
        vec![proc(1, "a.exe", 9999, 50.0)],
        false,
        vec![1],
    ));
    assert_eq!(count_kills(&d), 0);
}

#[test]
fn eased_pressure_releases_frozen_processes() {
    let d = decide(&snap_with(50.0, vec![proc(1, "a.exe", 9999, 1.0)], false, vec![1, 2]));
    let released: Vec<u32> = d
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::ReleaseProcess { pid, .. } => Some(*pid),
            _ => None,
        })
        .collect();
    assert_eq!(released, vec![1, 2]);
}

#[test]
fn still_high_pressure_keeps_processes_frozen() {
    let d = decide(&snap_with(
        93.0,
        vec![proc(1, "a.exe", 9999, 1.0)],
        false,
        vec![1],
    ));
    assert!(!d
        .actions
        .iter()
        .any(|a| matches!(a, Action::ReleaseProcess { .. })));
}

#[test]
fn release_never_targets_unfrozen_processes() {
    let d = decide(&snap_with(50.0, vec![proc(1, "a.exe", 9999, 1.0)], false, Vec::new()));
    assert!(!d
        .actions
        .iter()
        .any(|a| matches!(a, Action::ReleaseProcess { .. })));
}
