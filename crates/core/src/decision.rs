//! 决策阶梯:把「快照」映射为「动作清单」。纯函数,无副作用。
//! MVP-1 只输出决策,由调用方以干跑方式展示。

use crate::importance::classify;
use crate::metrics::{ProcessInfo, Snapshot};
use crate::pressure::{memory_used_percent, pressure_level, PressureLevel};
use crate::throttle::{throttle_plan, throttle_ratio};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// 清理待机(缓存)列表:不触碰任何进程。
    PurgeStandby { bytes: u64 },
    /// 修剪工作集:把页刷到页面文件,降低驻留内存。
    TrimWorkingSet { pid: u32, name: String, bytes: u64 },
    /// 按比例节流 CPU:`ratio` 为保留比率(0.0..=1.0)。
    ThrottleCpu { pid: u32, name: String, ratio: f32 },
    /// 极限手段:终止进程(仅在 critical 且启用时,且只针对后台进程)。
    KillProcess { pid: u32, name: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub level: PressureLevel,
    pub memory_used_percent: f32,
    pub cpu_used_percent: f32,
    pub actions: Vec<Action>,
    /// 供人读报告解释"为什么这样决策"。
    pub notes: Vec<String>,
}

/// 可削减的后台进程(过滤掉前台/系统/白名单)。
fn eligible(s: &Snapshot) -> Vec<ProcessInfo> {
    s.processes
        .iter()
        .filter(|p| classify(p, s).is_eligible())
        .cloned()
        .collect()
}

/// 综合得分:内存与 CPU 各占一半权重,用于挑选最该被终止的进程。
fn badness(p: &ProcessInfo, s: &Snapshot) -> f64 {
    let total_mem = s.memory.total_bytes.max(1) as f64;
    let mem_share = p.working_set_bytes as f64 / total_mem;
    let cpu_share = (p.cpu_percent as f64 / 100.0).min(1.0);
    mem_share * 0.5 + cpu_share * 0.5
}

pub fn decide(s: &Snapshot) -> Decision {
    let level = pressure_level(&s.memory, &s.config);
    let used = memory_used_percent(&s.memory);
    let mut actions = Vec::new();
    let mut notes = Vec::new();

    if level == PressureLevel::Idle {
        notes.push(format!("内存已用 {used}%,低于警戒线,无需动作"));
        return Decision {
            level,
            memory_used_percent: used,
            cpu_used_percent: s.cpu_used_percent,
            actions,
            notes,
        };
    }

    // L1:清待机列表(永远安全)
    actions.push(Action::PurgeStandby {
        bytes: s.memory.standby_bytes,
    });
    notes.push(format!("内存已用 {used}%,清理待机列表可回收缓存页"));

    let cand = eligible(s);
    let ratio = throttle_ratio(used, &s.config);

    // L3:按比例节流所有后台进程(压力达到 Warn 即生效)
    for (p, r) in throttle_plan(&cand, ratio) {
        actions.push(Action::ThrottleCpu {
            pid: p.pid,
            name: p.name.clone(),
            ratio: r,
        });
    }
    notes.push(format!(
        "对 {} 个后台进程按保留 {ratio:.2} 的比例节流",
        cand.len()
    ));

    // L2:High 及以上修剪工作集
    if level >= PressureLevel::High {
        let mut trimmed = 0usize;
        for p in &cand {
            if p.working_set_bytes >= s.config.min_working_set_bytes {
                actions.push(Action::TrimWorkingSet {
                    pid: p.pid,
                    name: p.name.clone(),
                    bytes: p.working_set_bytes,
                });
                trimmed += 1;
            }
        }
        notes.push(format!("修剪 {trimmed} 个后台进程的工作集"));
    }

    // L4:Critical 且启用时,选一个最该终止的进程
    if level >= PressureLevel::Critical && s.config.kill_enabled {
        if let Some(victim) = cand.iter().max_by(|a, b| {
            badness(a, s)
                .partial_cmp(&badness(b, s))
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            actions.push(Action::KillProcess {
                pid: victim.pid,
                name: victim.name.clone(),
                reason: format!("内存已用 {used}%,该进程综合占用最高"),
            });
            notes.push("已达临界级别:选出 1 个后台进程作为最后手段".into());
        }
    }

    Decision {
        level,
        memory_used_percent: used,
        cpu_used_percent: s.cpu_used_percent,
        actions,
        notes,
    }
}

#[cfg(test)]
mod tests;
