//! pressctl 命令行入口。
//! MVP-1 只做干跑:采集 -> 决策 -> 打印,**不执行任何动作**。

mod config;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "pressctl",
    version,
    about = "Windows 资源压力治理器(干跑模式:只决策,不执行)"
)]
struct Cli {
    /// 输出 JSON 而非人读文本
    #[arg(long)]
    json: bool,
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
    /// 允许在临界级别决策杀进程(本阶段仅决策,不执行)
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
        whitelist: cli.whitelist,
        kill_enabled: cli.kill_enabled,
    };

    let snap = pressctl_win::snapshot(args.to_policy());
    let decision = pressctl_core::decision::decide(&snap);

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
