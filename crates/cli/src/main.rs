//! pressctl 命令行入口。
//!
//! 默认是干跑(只打印决策)。只有当显式给出 `--apply` 或 `--watch` 时才会修改系统,
//! 且 `--watch` 退出前一定回滚所有可逆变更。

mod config;
mod state;
mod watch;

use clap::Parser;
use pressctl_win::exec::Journal;

#[derive(Parser, Debug)]
#[command(
    name = "pressctl",
    version,
    about = "Windows 资源压力治理器(默认干跑,不修改系统)"
)]
struct Cli {
    /// 输出 JSON 而非人读文本(仅干跑模式)
    #[arg(long)]
    json: bool,
    /// 一次性施加安全动作(清待机列表、修剪工作集)
    #[arg(long)]
    apply: bool,
    /// 常驻监视:循环决策并执行,退出时回滚
    #[arg(long)]
    watch: bool,
    /// 监视间隔秒数(配合 --watch)
    #[arg(long, default_value_t = 5)]
    interval: u64,
    /// 最多循环次数(用于验证;默认无限)
    #[arg(long)]
    max_iterations: Option<u32>,
    /// 恢复状态文件中记录的被冻结进程后退出
    #[arg(long)]
    release: bool,
    /// 警戒阈值(已用内存百分比)
    #[arg(long, default_value_t = 85.0)]
    warn: f32,
    /// 高压力阈值
    #[arg(long, default_value_t = 92.0)]
    high: f32,
    /// 临界阈值
    #[arg(long, default_value_t = 97.0)]
    critical: f32,
    /// 白名单可执行文件名(逗号分隔,永不触碰)
    #[arg(long, value_delimiter = ',')]
    whitelist: Vec<String>,
    /// 允许在持续临界时终止进程(最后手段)
    #[arg(long)]
    kill_enabled: bool,
}

fn main() {
    let cli = Cli::parse();
    let args = config::Args {
        json: cli.json,
        warn: cli.warn,
        high: cli.high,
        critical: cli.critical,
        whitelist: cli.whitelist.clone(),
        kill_enabled: cli.kill_enabled,
    };
    let policy = args.to_policy();

    if cli.release {
        std::process::exit(release_frozen());
    }

    if cli.watch {
        let code = watch::run(
            policy,
            watch::WatchArgs {
                interval_secs: cli.interval,
                max_iterations: cli.max_iterations,
            },
        );
        std::process::exit(code);
    }

    let snap = pressctl_win::snapshot(policy);
    let decision = pressctl_core::decision::decide(&snap);

    if cli.apply {
        let mut journal = Journal::new();
        watch::apply_one_shot(&mut journal, &decision.actions);
        println!("{}", journal.render());
        return;
    }

    // 默认:干跑
    if args.json {
        match pressctl_core::report::render_json(&decision, &snap) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("pressctl: 渲染 JSON 失败: {e}");
                std::process::exit(2);
            }
        }
    } else {
        print!("{}", pressctl_core::report::render_human(&decision, &snap));
    }
}

/// 恢复状态文件中记录的被冻结进程。
fn release_frozen() -> i32 {
    let pids = state::read();
    if pids.is_empty() {
        println!("pressctl: 没有需要恢复的进程");
        state::clear();
        return 0;
    }
    println!("pressctl: 恢复 {} 个被冻结的进程", pids.len());
    let mut failed = 0;
    for pid in pids {
        match pressctl_win::exec::life::resume(pid) {
            Ok(n) => println!("  - pid {pid}: 恢复 {n} 个线程"),
            Err(e) => {
                println!("  - pid {pid}: 失败 {e}");
                failed += 1;
            }
        }
    }
    state::clear();
    if failed > 0 {
        1
    } else {
        0
    }
}
