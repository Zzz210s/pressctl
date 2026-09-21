# pressctl

> Windows 资源压力治理器:当内存或 CPU 越过阈值时,按比例削减**后台**进程的占用。

[English](README.md) | 简体中文

## 目录

- [背景](#背景)
- [当前状态](#当前状态)
- [设计不变量](#设计不变量)
- [决策阶梯](#决策阶梯)
- [安装](#安装)
- [使用](#使用)
- [执行与安全](#执行与安全)
- [架构](#架构)
- [已知限制](#已知限制)
- [开发](#开发)
- [路线图](#路线图)
- [许可](#许可)

## 背景

16 GB 内存的机器上,已用内存长期高于 95%、提交量达到物理内存的两倍是常见状态。此时系统
会大量换页,所有应用都变得迟钝。

现有工具大致分两类:

- **内存清理类**(例如 Mem Reduct):能在阈值触发,但只清理缓存。
- **进程限制类**(例如 Process Governor、Process Lasso):施加的是**预先定好**的静态规则,
  不感知真实压力。

pressctl 针对的是两者之间的空白:一个**由反馈驱动**的治理器 —— 每次运行都读取真实压力,
据此决定该对哪些后台进程节流、修剪工作集、冻结,或(仅在最后手段时)终止。

## 当前状态

**默认模式是干跑:不修改任何状态。** 执行需要显式开启:`--apply`(一次性:清缓存与修剪工作集)
或 `--watch`(常驻:完整阶梯,包含 CPU 上限与冻结,退出时回滚)。

在已用内存 97.0% 的 16 GB 机器上实测:一次 `--apply` 修剪两个超阈值的进程后,
已用内存降到 91.9%(回收约 800 MB)。

## 设计不变量

以下不是可配置项,而是设计的性质。

- **永不设置内存硬上限。** 内存上限会让分配失败,进而导致应用崩溃。pressctl 只做修剪、
  节流与(可逆的)冻结。
- **永不触碰**:前台窗口进程及其**整个进程树**、Session 0 系统进程、交互宿主(shell、
  终端、桌面外壳),以及用户白名单中的任何进程。
- **优先使用可逆手段。** 阶梯依次升级为 节流 -> 修剪 -> 冻结,只有在用户显式允许、且
  冻结之后压力仍处临界时,才会终止一个进程。
- **压力缓解即释放。** 被冻结的进程会在压力回落到高阈值以下时恢复。

## 决策阶梯

| 内存压力 | 动作 |
| --- | --- |
| `Idle`(低于警戒线) | 无;并释放先前被冻结的进程 |
| `Warn` | 清理待机(缓存)列表;对所有可削减的后台进程按比例节流 |
| `High` | 上一级 + 修剪工作集超过下限的可削减进程 |
| `Critical` | 上一级 + 冻结综合占用最高的单个进程(可逆) |
| `Critical` 且已有冻结且 `--kill-enabled` | 终止一个进程作为最后手段 |

节流比率是**分段函数**而非线性映射:压力刚过警戒线时不过度压制,接近上限时又压制得足够。

| 已用内存 | 保留比率 |
| --- | --- |
| 低于 `warn` | 1.00(不节流) |
| 达到 `warn` | 0.75 |
| 达到 `high` | 0.50 |
| 达到 `critical` | 配置的下限(默认 0.25) |

## 安装

需要 Rust 工具链与 Windows。

```sh
git clone https://github.com/Zzz210s/pressctl.git
cd pressctl
cargo build --release
```

产物为 `target/release/pressctl.exe`(约 850 KB)。

## 使用

```sh
pressctl                 # 人读干跑报告
pressctl --json          # 机器可读报告
pressctl --whitelist node.exe,wallpaper64.exe
```

全部参数:

```
      --json                              输出 JSON 而非人读文本(仅干跑模式)
      --apply                             一次性施加安全动作(清待机列表、修剪工作集)
      --watch                             常驻监视:循环决策并执行,退出时回滚
      --interval <INTERVAL>               监视间隔秒数(配合 --watch)[默认: 5]
      --max-iterations <MAX_ITERATIONS>   最多循环次数(用于验证;默认无限)
      --release                           恢复状态文件中记录的被冻结进程后退出
      --warn <WARN>                       警戒阈值(已用内存百分比)[默认: 85]
      --high <HIGH>                       高压力阈值 [默认: 92]
      --critical <CRITICAL>               临界阈值 [默认: 97]
      --whitelist <WHITELIST>             白名单可执行文件名(逗号分隔,永不触碰)
      --kill-enabled                      允许在持续临界时终止进程(最后手段)
  -h, --help                              打印帮助
  -V, --version                           打印版本
```

在一台已用内存 95.6% 的机器上的真实输出:

```
=== pressctl (DRY-RUN, nothing is executed) ===
memory: used 95.6%  (703 MB free of 16069 MB)
cpu:    used 14.6%
level:  High
actions: 3
  - purge standby list (可回收约 0 MB)
  - throttle cpu: wetype_server.exe (pid 2696) -> keep 0.50
  - throttle cpu: PhoneExperienceHost.exe (pid 23780) -> keep 0.50
notes:
  - 内存已用 95.6%,清理待机列表可回收缓存页
  - 对 160 个后台进程按保留 0.50 的比例节流
  - 修剪 1 个后台进程的工作集
```

JSON 报告带有 `"schema_version": 1` 与 `"dry_run": true`,并包含受保护的前台 pid 集合,
便于核对豁免是否生效。

## 执行与安全

执行从不隐式发生。三种模式:

| 模式 | 做什么 | 可逆性 |
| --- | --- | --- |
| (默认) | 只打印决策 | 未施加任何变更 |
| `--apply` | 清理待机列表、修剪工作集;需要常驻的动作记为 skipped | 可逆 |
| `--watch` | 循环执行完整阶梯:CPU 上限、修剪、冻结,以及(仅在 `--kill-enabled` 时)终止 | 上限与冻结在退出时回滚 |

执行层的安全性质:

- **退出即回滚。** `--watch` 在退出前恢复所有被它冻结的进程、释放所有 CPU 上限,包括在
  Ctrl+C 时(控制台处理器把中断变成优雅停止)。
- **崩溃恢复。** 每次冻结都会把 pid 记入状态文件;若本工具被强杀,可用
  `pressctl --release` 恢复其中记录的全部进程。
- **CPU 上限依赖常驻。** 上限是运行中的进程持有的 Job Object,因此工具退出即释放。只有
  `--watch` 能在运行期间维持上限;`--apply` 会把它记为 skipped,而不是假装它持续生效。
- **终止不可逆**,因此它是唯一需要"持续临界 + 显式 `--kill-enabled`"双重条件的动作。

常驻运行施加上限并回滚的真实输出:

```
[1] used 91.9%  level Warn  actions 4
executed: 3 applied, 1 skipped, 0 failed
  - purge standby list: skipped: NtSetSystemInformation 返回 0xC0000061(通常为权限不足)
  - throttle rustc.exe (pid 15664) keep 0.75: applied: cpu rate capped to 75%
  - throttle tail.exe (pid 27252) keep 0.75: applied: cpu rate capped to 75%

rollback:
  - 已释放 3 个 CPU 上限
```

## 架构

三个 crate,依赖方向单一:

```
pressctl-core   纯逻辑:指标模型、压力分级、重要性分类、节流比率、决策阶梯、报告渲染。
                不依赖任何 Windows API,因此可全量单元测试。
      ^
      |
pressctl-win    仅采集:内存、进程列表、CPU 双点采样、前台窗口及其进程树。
                产出 core 的结构体,不做任何判断。
      ^
      |
pressctl-cli    参数解析、编排、输出。
```

由于决策是纯函数 `Snapshot -> Decision`,整条策略都能用构造数据测试,包括真实机器上难以
复现的状态(临界压力、已有冻结进程等)。

## 已知限制

- **清待机列表需要提权。** 该调用需要 `SeProfileSingleProcessPrivilege`;权限不足时记为
  skipped(而非 failed),不影响其他动作。
- **CPU 上限随工具退出而消失。** 它们是进程持有的 Job Object 句柄;要让上限持续生效,
  需保持 `--watch` 运行。
- **加入 Job 可能被拒绝。** 若目标进程已属于一个不允许嵌套的 Job,分配会失败;此种进程
  记为失败并保持原样。
- **冻结只挂起当下已存在的线程。** 之后新建的线程不会被挂起(使用的是文档化的
  `SuspendThread`,而非未文档化的 `NtSuspendProcess`)。
- **待机列表大小恒为 0。** 读取它需要未文档化的 `NtQuerySystemInformation`,为让其余指标
  永不失败而刻意延后。因此报告里的“可回收”数字为 0 MB。
- **每次运行有 300 ms 的 CPU 采样窗口开销** —— CPU 占用是速率,必须按差值测量。
- **父链可能断裂。** Windows 不会在父进程退出后重挂父链,因此由已退出进程启动的进程无法
  回溯到它的终端。pressctl 用内置的交互宿主名单(shell、终端、桌面外壳)缓解这一问题;但
  一个既孤立、又不在该名单上的进程会被当作后台处理。
- **需要存在前台窗口进程。** 当没有窗口处于前台时(例如锁屏),受保护集合为空。

## 开发

```sh
cargo test --workspace          # 59 个测试
cargo clippy --workspace --all-targets
```

每个源码文件都保持在 200 行以内;按职责拆分模块,而非按层次。

## 路线图

- **MVP-3** TOML 配置文件、常驻服务安装、已施加动作的历史记录。
- **MVP-4** 完善:占空比 CPU 节流(作为 Job Object 上限的备选)、完整进程树追溯、
  可配置的保护名单。

## 许可

以下两种许可任选其一:

- Apache License, Version 2.0([LICENSE-APACHE](LICENSE-APACHE))
- MIT license([LICENSE-MIT](LICENSE-MIT))
