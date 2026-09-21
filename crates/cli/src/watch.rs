//! 常驻监视模式:循环 采集 -> 决策 -> 执行,并在退出时**回滚全部变更**。
//!
//! 这是 CPU 上限与冻结唯一有意义的运行方式:Job 句柄必须由本进程持有,
//! 被冻结的进程也必须有人负责恢复。

use crate::state;
use pressctl_core::metrics::PolicyConfig;
use pressctl_win::exec::Journal;

pub struct WatchArgs {
    pub interval_secs: u64,
    pub max_iterations: Option<u32>,
}

/// 单次施加"一次性安全"的动作(清待机列表、修剪工作集)。
/// 需要常驻的动作会被记为 skipped,并说明原因。
pub fn apply_one_shot(journal: &mut Journal, actions: &[pressctl_core::decision::Action]) {
    for a in actions {
        match a {
            pressctl_core::decision::Action::PurgeStandby { .. }
            | pressctl_core::decision::Action::TrimWorkingSet { .. } => journal.apply(a),
            _ => journal.skip(a, "需要常驻模式(--watch)才能维持该动作"),
        }
    }
}

/// 常驻循环。返回进程退出码(0 = 正常,1 = 有失败项)。
pub fn run(policy: PolicyConfig, args: WatchArgs) -> i32 {
    if let Err(e) = pressctl_win::console::install_stop_handler() {
        eprintln!("pressctl: 无法安装 Ctrl+C 处理器: {e}(冻结将无法回滚,请谨慎)");
    }

    let mut journal = Journal::new();
    let mut iteration = 0u32;

    loop {
        if pressctl_win::console::stop_requested() {
            println!("\npressctl: 收到停止请求,开始回滚");
            break;
        }

        let snap = pressctl_win::snapshot(policy.clone());
        let decision = pressctl_core::decision::decide(&snap);
        iteration += 1;

        println!(
            "[{iteration}] used {:.1}%  level {:?}  actions {}",
            decision.memory_used_percent,
            decision.level,
            decision.actions.len()
        );
        for a in &decision.actions {
            journal.apply(a);
        }

        // 冻结集合落盘,便于被强杀后用 --release 恢复
        if let Err(e) = state::write(journal.frozen_pids()) {
            eprintln!("pressctl: {e}");
        }

        if let Some(max) = args.max_iterations {
            if iteration >= max {
                break;
            }
        }
        sleep_interruptible(args.interval_secs);
    }

    let notes = journal.rollback();
    state::clear();
    println!("{}", journal.render());
    if !notes.is_empty() {
        println!("rollback:");
        for n in &notes {
            println!("  - {n}");
        }
    }
    if journal.failed > 0 {
        1
    } else {
        0
    }
}

/// 可被 Ctrl+C 打断的休眠:切成 100 ms 小片,保证响应速度。
fn sleep_interruptible(secs: u64) {
    for _ in 0..(secs * 10) {
        if pressctl_win::console::stop_requested() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
