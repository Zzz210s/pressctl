# pressctl

> A Windows resource-pressure governor: when memory or CPU crosses a threshold, it proportionally reduces the footprint of **background** processes.

English | [简体中文](README.zh-CN.md)

## Table of Contents

- [Background](#background)
- [Current Status](#current-status)
- [Design Invariants](#design-invariants)
- [Decision Ladder](#decision-ladder)
- [Install](#install)
- [Usage](#usage)
- [Execution and Safety](#execution-and-safety)
- [Architecture](#architecture)
- [Limitations](#limitations)
- [Development](#development)
- [Roadmap](#roadmap)
- [License](#license)

## Background

On a 16 GB machine it is common for memory usage to sit above 95% while the pagefile absorbs
the difference (a commit charge of roughly twice the physical RAM). When that happens the
system swaps heavily and every application becomes sluggish.

Existing tools tend to fall into two camps:

- **Memory cleaners** (for example Mem Reduct) trigger on a threshold, but only trim caches.
- **Process limiters** (for example Process Governor, Process Lasso) apply explicit, static
  rules that are decided in advance; they do not react to actual pressure.

pressctl targets the gap between them: a **feedback-driven** governor that reads real pressure
and decides, per run, which background processes should be throttled, trimmed, frozen or — as a
last resort only — terminated.

## Current Status

**The default mode is a dry run: nothing is modified.** Execution is opt-in through `--apply`
(one-shot: purge caches and trim working sets) or `--watch` (resident: the full ladder,
including CPU caps and process freezing, with rollback on exit).

Measured effect of a single `--apply` run on a 16 GB machine sitting at 97.0% memory usage:
usage dropped to 91.9% (about 800 MB reclaimed) after trimming two processes above the size
floor.

## Design Invariants

These are not configurable; they are properties of the design.

- **No hard memory limits, ever.** Memory caps make allocations fail, which crashes
  applications. pressctl only trims, throttles and (reversibly) freezes.
- **Never touch** the foreground window process and its whole process tree, Session 0 system
  processes, interactive hosts (shells, terminals, the desktop shell) or anything on the
  user's whitelist.
- **Prefer reversible actions.** The ladder escalates throttle -> trim -> freeze, and only
  terminates a process when the user explicitly allows it and pressure stays critical after a
  freeze has already been applied.
- **Released when it eases.** Processes that were frozen are resumed once pressure drops back
  below the high threshold.

## Decision Ladder

| Memory pressure | Actions |
| --- | --- |
| `Idle` (below warn) | none; release any previously frozen process |
| `Warn` | purge the standby (cache) list; throttle every eligible background process proportionally |
| `High` | previous level, plus trim the working set of eligible processes above a size floor |
| `Critical` | previous level, plus freeze the single worst offender (reversible) |
| `Critical`, already frozen, and `--kill-enabled` | terminate one process as a last resort |

The throttle ratio is a step function rather than a linear one, so that pressure just above the
warning line is not over-corrected while pressure near the limit is still corrected enough:

| Used memory | Keep ratio |
| --- | --- |
| below `warn` | 1.00 (no throttle) |
| at or above `warn` | 0.75 |
| at or above `high` | 0.50 |
| at or above `critical` | the configured floor (default 0.25) |

## Install

Requires a Rust toolchain and Windows.

```sh
git clone https://github.com/Zzz210s/pressctl.git
cd pressctl
cargo build --release
```

The binary is written to `target/release/pressctl.exe` (about 850 KB).

## Usage

```sh
pressctl                 # human-readable dry-run report
pressctl --json          # machine-readable report
pressctl --whitelist node.exe,wallpaper64.exe
```

All options:

```
      --json                              Output JSON instead of human-readable text (dry run only)
      --apply                             Apply the one-shot safe actions (purge standby, trim working sets)
      --watch                             Resident mode: decide and act in a loop, roll back on exit
      --interval <INTERVAL>               Seconds between watch iterations [default: 5]
      --max-iterations <MAX_ITERATIONS>   Stop after N iterations (for verification; default unlimited)
      --release                           Resume processes recorded in the state file, then exit
      --warn <WARN>                       Warning threshold (used memory percent) [default: 85]
      --high <HIGH>                       High pressure threshold [default: 92]
      --critical <CRITICAL>               Critical threshold [default: 97]
      --whitelist <WHITELIST>             Whitelisted executable names (comma separated, never touched)
      --kill-enabled                      Allow terminating a process at sustained critical pressure
  -h, --help                              Print help
  -V, --version                           Print version
```

Example output on a machine sitting at 95.6% memory usage:

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

The JSON report carries `"schema_version": 1` and `"dry_run": true`, and includes the protected
foreground pid set so that exemptions can be verified.

## Execution and Safety

Execution is never implicit. Three opt-in modes exist:

| Mode | What it does | Reversible |
| --- | --- | --- |
| (default) | prints the decision only | nothing applied |
| `--apply` | purges the standby list and trims working sets; actions that need residency are recorded as skipped | yes |
| `--watch` | runs the full ladder in a loop: CPU caps, trimming, freezing, and (only with `--kill-enabled`) termination | caps and freezes are rolled back on exit |

Safety properties of the execution layer:

- **Rollback on exit.** `--watch` resumes every process it froze and releases every CPU cap
  before exiting, including on Ctrl+C (a console control handler turns the interrupt into a
  graceful stop).
- **Crash recovery.** Frozen pids are written to a state file as they are applied. If the tool
  is killed forcefully, `pressctl --release` resumes everything recorded there.
- **CPU caps need residency.** A cap is a Job Object held by the running process, so it is
  released when the tool exits. Only `--watch` keeps caps applied while it runs; `--apply`
  records them as skipped rather than pretending they persisted.
- **Termination is not reversible** and is therefore the only action that requires both
  sustained critical pressure and an explicit `--kill-enabled`.

Example of a resident run that applies caps and then rolls them back:

```
[1] used 91.9%  level Warn  actions 4
executed: 3 applied, 1 skipped, 0 failed
  - purge standby list: skipped: NtSetSystemInformation returned 0xC0000061 (usually insufficient privileges)
  - throttle rustc.exe (pid 15664) keep 0.75: applied: cpu rate capped to 75%
  - throttle tail.exe (pid 27252) keep 0.75: applied: cpu rate capped to 75%

rollback:
  - released 3 CPU caps
```

## Architecture

Three crates with a single dependency direction:

```
pressctl-core   pure logic: metrics model, pressure levels, importance classification,
                throttle ratios, decision ladder, report rendering. No Windows API,
                therefore fully unit-testable.
      ^
      |
pressctl-win    collection only: memory, process list, CPU sampling (two-point delta),
                foreground window and its process tree. Produces core structs; makes
                no decisions.
      ^
      |
pressctl-cli    argument parsing, orchestration, output.
```

Because the decision function is a pure `Snapshot -> Decision` transformation, the whole policy
can be tested with synthetic data, including states (critical pressure, already-frozen
processes) that are hard to reproduce on a real machine.

## Limitations

- **Purging the standby list needs elevation.** The call requires
  `SeProfileSingleProcessPrivilege`; without it the action is recorded as skipped (not failed)
  and nothing else is affected.
- **CPU caps vanish when the tool exits.** They are Job Object handles owned by the process.
  Use `--watch` for as long as you want the caps to hold.
- **Job assignment can be refused.** Assigning a process that already belongs to a job that
  does not allow nesting fails; such a process is reported as a failure and left untouched.
- **Freezing suspends the threads that exist at that moment.** Threads created afterwards are
  not suspended (the documented `SuspendThread` path is used rather than the undocumented
  `NtSuspendProcess`).
- **Standby list size is always reported as 0.** Reading it requires the undocumented
  `NtQuerySystemInformation`; it is deliberately deferred so that the remaining metrics never
  fail. The "reclaimable" figure in the report is therefore 0 MB.
- **CPU sampling costs a 300 ms window** per run, because CPU usage is a rate and must be
  measured as a delta.
- **Broken parent chains.** Windows does not reparent orphans, so a process whose launching
  process has exited cannot be traced back to its terminal. pressctl mitigates this with a
  built-in interactive-host list (shells, terminals, the desktop shell), but a process that is
  both orphaned and not on that list is treated as background.
- **The foreground window process must exist.** When no window is in the foreground (for
  example during a locked session) the protected set is empty.

## Development

```sh
cargo test --workspace          # 59 tests
cargo clippy --workspace --all-targets
```

Every source file is kept under 200 lines; responsibilities are split by module rather than by
layer.

## Roadmap

- **MVP-3** TOML configuration file, resident service installation, and history of applied
  actions.
- **MVP-4** refinements: duty-cycle CPU throttling as an alternative to Job Object caps,
  complete process tree tracing, configurable protection lists.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
