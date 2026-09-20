//! 报告渲染:把决策翻译成人读文本或 JSON。
//! 本阶段所有报告都标注 DRY-RUN,以明确"未执行任何动作"。

pub mod human;
pub mod json;

pub use human::render_human;
pub use json::render_json;
