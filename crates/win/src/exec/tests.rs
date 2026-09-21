//! exec 模块的单元测试(独立文件以保持主文件在 200 行以内)。

use super::Journal;
use pressctl_core::decision::Action;

    #[test]
    fn trimming_own_process_is_applied() {
        let mut j = Journal::new();
        j.apply(&Action::TrimWorkingSet {
            pid: std::process::id(),
            name: "pressctl-test".into(),
            bytes: 0,
        });
        assert_eq!(j.applied, 1, "{}", j.render());
        assert_eq!(j.failed, 0);
    }

    #[test]
    fn trimming_a_bogus_pid_is_recorded_as_failure() {
        let mut j = Journal::new();
        j.apply(&Action::TrimWorkingSet {
            pid: 0xFFFF_FFF0,
            name: "ghost".into(),
            bytes: 0,
        });
        assert_eq!(j.failed, 1);
        assert!(j.entries()[0].1.is_failed());
    }

    #[test]
    fn freeze_then_rollback_resumes_the_process() {
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "ping", "-n", "20", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn");
        std::thread::sleep(std::time::Duration::from_millis(300));

        let mut j = Journal::new();
        j.apply(&Action::FreezeProcess {
            pid: child.id(),
            name: "child".into(),
            reason: "test".into(),
        });
        assert_eq!(j.applied, 1, "{}", j.render());
        assert_eq!(j.frozen_pids(), vec![child.id()]);

        let notes = j.rollback();
        assert!(j.frozen_pids().is_empty());
        assert!(!notes.is_empty(), "回滚应产生说明");

        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn killed_processes_are_recorded_and_not_rolled_back() {
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "ping", "-n", "20", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn");
        std::thread::sleep(std::time::Duration::from_millis(300));

        let mut j = Journal::new();
        j.apply(&Action::KillProcess {
            pid: child.id(),
            name: "child".into(),
            reason: "test".into(),
        });
        assert_eq!(j.applied, 1, "{}", j.render());
        assert_eq!(j.killed, vec![child.id()]);
        let _ = child.wait();
        assert!(j.rollback().is_empty(), "终止不可回滚");
    }
