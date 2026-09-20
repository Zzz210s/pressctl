//! 采集结果的数据模型。字段全部使用字节(而非 MB)以避免精度歧义。

use serde::{Deserialize, Serialize};

/// 物理内存与提交量指标。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub commit_total_bytes: u64,
    pub commit_limit_bytes: u64,
    /// 待机(缓存)列表占用,清理它不会影响任何进程。
    pub standby_bytes: u64,
}

/// 单个进程的观测值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    /// 可执行文件名(不含路径),用于白名单匹配。
    pub name: String,
    pub working_set_bytes: u64,
    pub private_bytes: u64,
    /// CPU 占用百分比,0.0..=100.0(已按逻辑核数归一)。
    pub cpu_percent: f32,
    /// Windows 会话 ID;0 表示系统会话。
    pub session_id: u32,
}

/// 策略配置(阈值与豁免)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub warn_percent: f32,
    pub high_percent: f32,
    pub critical_percent: f32,
    pub cpu_high_percent: f32,
    /// 小于此工作集的进程不值得修剪。
    pub min_working_set_bytes: u64,
    /// 节流比率下限:0.25 表示最多把某进程压到原有速度的 25%。
    pub cpu_throttle_floor: f32,
    /// 是否允许在 critical 级别决策杀进程(MVP-1 仅决策,不执行)。
    pub kill_enabled: bool,
    /// 白名单:永不触碰的可执行文件名(小写比较)。
    pub whitelist: Vec<String>,
}

/// 一次完整观测。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub memory: MemoryMetrics,
    pub processes: Vec<ProcessInfo>,
    /// 前台窗口进程及其进程树内的所有 pid。
    pub foreground_pids: Vec<u32>,
    /// 当前已被本工具挂起(冻结)的 pid 集合。
    /// 用于在压力缓解后恢复,以及判断"冻结已无效,是否升级为终止"。
    pub frozen_pids: Vec<u32>,
    /// 整机 CPU 占用百分比(0.0..=100.0)。
    pub cpu_used_percent: f32,
    pub config: PolicyConfig,
}
