//! JSON 报告:供脚本/历史对比消费。结构带 schema_version 以便后续演进。

use crate::decision::Decision;
use crate::metrics::Snapshot;
use serde_json::json;

pub fn render_json(d: &Decision, s: &Snapshot) -> Result<String, serde_json::Error> {
    let value = json!({
        "schema_version": crate::SCHEMA_VERSION,
        "dry_run": true,
        "snapshot": {
            "memory": s.memory,
            "cpu_used_percent": s.cpu_used_percent,
            "process_count": s.processes.len(),
            "foreground_pids": s.foreground_pids,
            "config": s.config,
        },
        "decision": d,
    });
    serde_json::to_string_pretty(&value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::Action;
    use crate::metrics::{MemoryMetrics, PolicyConfig};
    use crate::pressure::PressureLevel;

    fn snap() -> Snapshot {
        Snapshot {
            memory: MemoryMetrics {
                total_bytes: 100,
                available_bytes: 10,
                commit_total_bytes: 100,
                commit_limit_bytes: 200,
                standby_bytes: 5,
            },
            processes: Vec::new(),
            foreground_pids: Vec::new(),
            frozen_pids: Vec::new(),
            cpu_used_percent: 1.0,
            config: PolicyConfig {
                warn_percent: 85.0,
                high_percent: 92.0,
                critical_percent: 97.0,
                cpu_high_percent: 90.0,
                min_working_set_bytes: 0,
                cpu_throttle_floor: 0.25,
                kill_enabled: true,
                whitelist: vec!["keep.exe".into()],
            },
        }
    }

    #[test]
    fn json_is_valid_and_carries_schema_version() {
        let d = Decision {
            level: PressureLevel::Warn,
            memory_used_percent: 90.0,
            cpu_used_percent: 1.0,
            actions: vec![Action::PurgeStandby { bytes: 5 }],
            notes: Vec::new(),
        };
        let s = render_json(&d, &snap()).expect("json");
        let v: serde_json::Value = serde_json::from_str(&s).expect("parse");
        assert_eq!(v["schema_version"], crate::SCHEMA_VERSION);
        assert_eq!(v["decision"]["level"], "Warn");
    }

    #[test]
    fn json_marks_dry_run() {
        let d = Decision {
            level: PressureLevel::Idle,
            memory_used_percent: 1.0,
            cpu_used_percent: 1.0,
            actions: Vec::new(),
            notes: Vec::new(),
        };
        let v: serde_json::Value =
            serde_json::from_str(&render_json(&d, &snap()).expect("json")).expect("parse");
        assert_eq!(v["dry_run"], true);
    }
}
