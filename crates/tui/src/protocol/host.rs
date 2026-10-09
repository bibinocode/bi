//! 定义组件向宿主请求重绘、投递消息和管理资源的接口

use std::sync::mpsc::Sender;

use super::{ComponentError, ComponentHandle, ComponentResult};

/// 当前组件实例拥有的资源标识。
///
/// 标识只在所属实例内有效。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(pub u64);

/// 释放组件拥有的外部资源。
///
/// 例如取消定时器、取消任务或移除事件监听器。
/// 不要求 Send，允许捕获单线程脚本运行时的资源。
pub type ResourceCleanup = Box<dyn FnOnce() + 'static>;

/// 等待宿主检查并派发的消息。
///
/// source 和 target 都必须仍然有效，才能派发。
#[derive(Debug)]
pub struct QueuedMessage {
    pub source: ComponentHandle,
    pub target: ComponentHandle,
    pub topic: String,
    pub payload: Vec<u8>,
}

/// 可被异步任务持有的消息发送器。
///
/// 它不直接访问组件，也不直接修改 UI。
#[derive(Debug, Clone)]
pub struct MessageSender {
    source: ComponentHandle,
    sender: Sender<QueuedMessage>,
}

impl MessageSender {
    /// 由宿主适配层创建。
    ///
    /// 接收队列的一方负责验证实例并派发消息。
    pub fn new(source: ComponentHandle, sender: Sender<QueuedMessage>) -> Self {
        Self { source, sender }
    }

    pub fn source(&self) -> ComponentHandle {
        self.source
    }

    /// 将消息加入队列。
    ///
    /// 成功只代表入队成功，不代表目标组件已经处理。
    /// 实例失效时，宿主会丢弃消息。
    pub fn post(
        &self,
        target: ComponentHandle,
        topic: impl Into<String>,
        payload: impl Into<Vec<u8>>,
    ) -> ComponentResult<()> {
        self.sender
            .send(QueuedMessage {
                source: self.source,
                target,
                topic: topic.into(),
                payload: payload.into(),
            })
            .map_err(|_| ComponentError::OperationFailed {
                message: "UI message queue is closed".into(),
            })
    }
}

/// 宿主授予一个具体组件实例的能力。
///
/// 所有调用都属于 handle() 返回的实例。
/// 宿主实现必须检查该实例的生命周期状态。
pub trait ComponentHost {
    fn handle(&self) -> ComponentHandle;

    /// 请求重新布局并绘制。
    ///
    /// 宿主可以合并请求。
    /// 已失效的实例不能影响当前 UI。
    fn request_redraw(&mut self) -> ComponentResult<()>;

    /// 获取绑定当前实例的异步消息发送器。
    fn message_sender(&self) -> MessageSender;

    /// 向组件投递消息。
    ///
    /// 与 MessageSender::post 一样，成功仅表示入队。
    fn post_message(
        &mut self,
        target: ComponentHandle,
        topic: String,
        payload: Vec<u8>,
    ) -> ComponentResult<()> {
        self.message_sender().post(target, topic, payload)
    }

    /// 将资源清理责任交给宿主。
    ///
    /// 组件卸载、替换或候选实例加载失败时，
    /// 宿主必须执行尚未释放的清理函数。
    ///
    /// 如果登记失败，宿主必须立即执行传入的清理函数，
    /// 避免调用方失去清理函数后发生资源泄漏。
    fn register_cleanup(&mut self, cleanup: ResourceCleanup) -> ComponentResult<ResourceId>;

    /// 提前释放当前实例拥有的资源。
    ///
    /// 成功释放后，该清理函数不会在卸载时重复执行。
    /// 当前实例不能释放其他实例拥有的资源。
    fn release_resource(&mut self, resource: ResourceId) -> ComponentResult<()>;
}
