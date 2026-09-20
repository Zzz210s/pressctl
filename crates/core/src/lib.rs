//! pressctl 的纯逻辑层:指标模型、压力分级、重要性分类、决策与报告渲染。
//! 本 crate 不依赖任何 Windows API,因此全部逻辑可用单元测试覆盖。

pub const SCHEMA_VERSION: u32 = 1;

pub mod metrics;
pub mod pressure;
