//! 执行结果与动作标签:把"做了什么"渲染成人读文本。

use pressctl_core::decision::Action;
use serde::{Deserialize, Serialize};

/// 单个动作的执行结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    Applied(String),
    Skipped(String),
    Failed(String),
}

impl Outcome {
    pub fn describe(&self) -> String {
        match self {
            Outcome::Applied(s) => format!("applied: {s}"),
            Outcome::Skipped(s) => format!("skipped: {s}"),
            Outcome::Failed(s) => format!("FAILED: {s}"),
        }
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Outcome::Failed(_))
    }
}

/// 动作的人读标签(与具体执行路径无关,便于 skip 与 apply 保持一致)。
pub fn label(a: &Action) -> String {
    match a {
        Action::PurgeStandby { .. } => "purge standby list".to_string(),
        Action::TrimWorkingSet { pid, name, bytes } => {
            format!("trim {name} (pid {pid}, {:.0} MB)", mb(*bytes))
        }
        Action::ThrottleCpu { pid, name, ratio } => {
            format!("throttle {name} (pid {pid}) keep {ratio:.2}")
        }
        Action::FreezeProcess { pid, name, .. } => format!("freeze {name} (pid {pid})"),
        Action::ReleaseProcess { pid, name } => format!("release {name} (pid {pid})"),
        Action::KillProcess { pid, name, .. } => format!("kill {name} (pid {pid})"),
    }
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}
