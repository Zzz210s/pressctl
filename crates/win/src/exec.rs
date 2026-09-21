//! 执行层:把决策中的动作施加到系统,并登记可回滚的变更。
//! 除终止外所有动作都可逆:修剪与清缓存是一次性的,CPU 上限与冻结由登记簿回滚。

pub mod cpu;
pub mod life;
pub mod mem;
pub mod outcome;

pub use outcome::{label, Outcome};

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

use pressctl_core::decision::Action;

/// 已施加动作的登记簿。持有 CPU 限制句柄与被冻结的 pid,因此支持回滚。
#[derive(Default)]
pub struct Journal {
    entries: Vec<(String, Outcome)>,
    frozen: Vec<u32>,
    caps: Vec<cpu::CpuCap>,
    /// 被终止的 pid(不可回滚,仅作记录)
    pub killed: Vec<u32>,
    /// 汇总计数
    pub applied: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl Journal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[(String, Outcome)] {
        &self.entries
    }

    pub fn frozen_pids(&self) -> &[u32] {
        &self.frozen
    }

    /// 某进程是否已被冻结。
    pub fn is_frozen(&self, pid: u32) -> bool {
        self.frozen.contains(&pid)
    }

    /// 某进程当前的 CPU 保留比率。
    pub fn cap_ratio(&self, pid: u32) -> Option<f32> {
        self.caps.iter().find(|c| c.pid == pid).map(|c| c.ratio)
    }

    /// 记录一个"本次未执行"的动作(例如一次性运行无法维持 CPU 上限)。
    pub fn skip(&mut self, action: &Action, reason: &str) {
        self.record(label(action), Outcome::Skipped(reason.to_string()));
    }

    fn record(&mut self, label: String, outcome: Outcome) {
        match &outcome {
            Outcome::Applied(_) => self.applied += 1,
            Outcome::Failed(_) => self.failed += 1,
            Outcome::Skipped(_) => self.skipped += 1,
        }
        self.entries.push((label, outcome));
    }

    /// 施加一个动作。`PurgeStandby` 在无权限时记为 skipped 而非 failed。
    pub fn apply(&mut self, action: &Action) {
        match action {
            Action::PurgeStandby { .. } => {
                let label = "purge standby list".to_string();
                match mem::purge_standby() {
                    Ok(()) => self.record(label, Outcome::Applied("standby list purged".into())),
                    Err(e) => self.record(label, Outcome::Skipped(e)),
                }
            }
            Action::TrimWorkingSet { pid, name, bytes } => {
                let label = format!("trim {name} (pid {pid}, {:.0} MB)", mb(*bytes));
                match mem::trim_working_set(*pid) {
                    Ok(()) => self.record(label, Outcome::Applied("working set trimmed".into())),
                    Err(e) => self.record(label, Outcome::Failed(e)),
                }
            }
            Action::ThrottleCpu { pid, ratio, .. } => {
                let label = label(action);
                if self.cap_ratio(*pid) == Some(*ratio) {
                    self.record(label, Outcome::Skipped("already capped at this ratio".into()));
                    return;
                }
                match cpu::CpuCap::apply(*pid, *ratio) {
                    Ok(cap) => {
                        self.caps.push(cap);
                        self.record(
                            label,
                            Outcome::Applied(format!("cpu rate capped to {:.0}%", ratio * 100.0)),
                        );
                    }
                    Err(e) => self.record(label, Outcome::Failed(e)),
                }
            }
            Action::FreezeProcess { pid, .. } => {
                let label = label(action);
                if self.is_frozen(*pid) {
                    self.record(label, Outcome::Skipped("already frozen".into()));
                    return;
                }
                match life::suspend(*pid) {
                    Ok(n) => {
                        self.frozen.push(*pid);
                        self.record(label, Outcome::Applied(format!("{n} threads suspended")));
                    }
                    Err(e) => self.record(label, Outcome::Failed(e)),
                }
            }
            Action::ReleaseProcess { pid, name } => {
                let label = format!("release {name} (pid {pid})");
                match life::resume(*pid) {
                    Ok(n) => {
                        self.frozen.retain(|p| p != pid);
                        self.record(label, Outcome::Applied(format!("{n} threads resumed")));
                    }
                    Err(e) => self.record(label, Outcome::Failed(e)),
                }
            }
            Action::KillProcess { pid, name, reason } => {
                let label = format!("kill {name} (pid {pid})");
                match life::terminate(*pid) {
                    Ok(()) => {
                        self.killed.push(*pid);
                        self.record(label, Outcome::Applied(format!("terminated ({reason})")));
                    }
                    Err(e) => self.record(label, Outcome::Failed(e)),
                }
            }
        }
    }

    /// 回滚:恢复所有被冻结的进程,并释放所有 CPU 上限。
    pub fn rollback(&mut self) -> Vec<String> {
        let mut notes = Vec::new();
        for pid in std::mem::take(&mut self.frozen) {
            match life::resume(pid) {
                Ok(n) => notes.push(format!("已恢复 pid {pid} 的 {n} 个线程")),
                Err(e) => notes.push(format!("恢复 pid {pid} 失败: {e}")),
            }
        }
        let caps = std::mem::take(&mut self.caps);
        if !caps.is_empty() {
            notes.push(format!("已释放 {} 个 CPU 上限", caps.len()));
        }
        drop(caps);
        notes
    }

    /// 人读摘要。
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "executed: {} applied, {} skipped, {} failed\n",
            self.applied, self.skipped, self.failed
        ));
        for (label, outcome) in &self.entries {
            out.push_str(&format!("  - {label}: {}\n", outcome.describe()));
        }
        out
    }
}

#[cfg(test)]
mod tests;
