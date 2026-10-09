use crate::component::{Component, ComponentNode, LayoutSnapshot};
use crate::protocol::{
    ComponentError, ComponentEvent, ComponentHost, ComponentResult, EventResponse, LayoutContext,
    PointerEvent,
};

use super::Container;

type MouseHandler =
    Box<dyn FnMut(&PointerEvent, &mut dyn ComponentHost) -> ComponentResult<EventResponse>>;

/// 给一个运行时子节点添加鼠标处理，不改变子节点的生命周期或焦点身份。
/// 子节点先收到事件；只有未消费的事件才沿运行时的冒泡路径到达本组件。
/// 回调坐标相对于 MouseRegion，允许通过响应请求重绘、捕获和释放指针。
pub struct MouseRegion {
    handler: MouseHandler,
}

impl MouseRegion {
    pub fn new(
        handler: impl FnMut(&PointerEvent, &mut dyn ComponentHost) -> ComponentResult<EventResponse>
        + 'static,
    ) -> Self {
        Self {
            handler: Box::new(handler),
        }
    }
}

impl Component for MouseRegion {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        if children.len() != 1 {
            return Err(ComponentError::InvalidLayout {
                reason: "MouseRegion requires exactly one child".into(),
            });
        }
        Container::new().layout(context, children)
    }

    fn handle_event(
        &mut self,
        event: &ComponentEvent,
        host: &mut dyn ComponentHost,
    ) -> ComponentResult<EventResponse> {
        match event {
            ComponentEvent::Pointer(pointer) => (self.handler)(pointer, host),
            _ => Ok(EventResponse::default()),
        }
    }
}
