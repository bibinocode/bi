//! 定义组件热重载时交换的状态快照

/// 组件导出的可恢复状态。
///
/// 组件负责序列化、校验、迁移和恢复。
/// 宿主负责在组件替换流程中传递快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSnapshot {
    /// 当前组件状态格式的版本。
    ///
    /// 与 ComponentHandle.generation 不同：
    /// schema_version 描述数据格式，
    /// generation 描述组件实例。
    pub schema_version: u32,

    /// payload 的编码约定。
    ///
    /// 例如：
    /// application/json
    /// text/plain;charset=utf-8
    ///
    /// 仅声明格式，不会自动执行编码或解码。
    pub encoding: String,

    /// 序列化后的状态数据。
    ///
    /// 不包含指向旧组件实例的对象引用。
    pub payload: Vec<u8>,
}
