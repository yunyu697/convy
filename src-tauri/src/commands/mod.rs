//! IPC 命令层。
//!
//! 每个子模块对应界面上的一块功能区，只做「参数校验 + 状态读写 + 调用领域模块」，
//! 不承载业务逻辑 —— 真正的执行逻辑在 `crate::queue`、`crate::engines` 等模块里。
//!
//! 拆分的直接动机：这些命令原来全堆在 `lib.rs`（1130 行、30 个命令），
//! 找一条命令要先在文件里翻半天，而它们之间其实没有耦合。

pub mod engine;
pub mod history;
pub mod queue;
pub mod settings;
pub mod system;
pub mod update;
