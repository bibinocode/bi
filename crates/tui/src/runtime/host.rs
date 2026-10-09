use std::sync::mpsc::Sender;

use crate::protocol::{
    ComponentError, ComponentHandle, ComponentHost, ComponentResult, MessageSender, QueuedMessage,
    ResourceCleanup, ResourceId,
};

use super::{ComponentRegistry, RedrawState, ResourceTable};

/// 绑定到一个组件实例的临时运行时服务入口。
///
/// 不保存组件对象，也不直接处理组件事件。
pub struct RuntimeHost<'a> {
    handle: ComponentHandle,
    registry: &'a ComponentRegistry,
    resources: &'a mut ResourceTable,
    redraw: &'a mut RedrawState,
    messages: MessageSender,
}

impl<'a> RuntimeHost<'a> {
    pub fn new(
        handle: ComponentHandle,
        registry: &'a ComponentRegistry,
        resources: &'a mut ResourceTable,
        redraw: &'a mut RedrawState,
        sender: Sender<QueuedMessage>,
    ) -> Self {
        Self {
            handle,
            registry,
            resources,
            redraw,
            messages: MessageSender::new(handle, sender),
        }
    }
}

impl ComponentHost for RuntimeHost<'_> {
    fn handle(&self) -> ComponentHandle {
        self.handle
    }

    fn request_redraw(&mut self) -> ComponentResult<()> {
        if !self.registry.is_active(self.handle) && !self.registry.is_pending(self.handle) {
            return Err(ComponentError::OperationFailed {
                message: "inactive component cannot request redraw".into(),
            });
        }

        self.redraw.request();

        Ok(())
    }

    fn message_sender(&self) -> MessageSender {
        self.messages.clone()
    }

    fn register_cleanup(&mut self, cleanup: ResourceCleanup) -> ComponentResult<ResourceId> {
        self.resources.register(self.registry, self.handle, cleanup)
    }

    fn release_resource(&mut self, id: ResourceId) -> ComponentResult<()> {
        self.resources.release(self.handle, id)
    }
}
