use crate::protocol::{
    ComponentError, ComponentEvent, ComponentHost, ComponentResult, EventResponse, LayoutContext,
    StateSnapshot,
};

use super::{ComponentNode, LayoutSnapshot};

pub trait Component {
    /// 是否参加宿主默认的 Tab/Shift+Tab 焦点遍历。
    fn focusable(&self) -> bool {
        false
    }

    /// 组件挂载后调用，可通过 host 注册需要清理的资源。
    fn mount(&mut self, _host: &mut dyn ComponentHost) -> ComponentResult<()> {
        Ok(())
    }

    /// 计算布局并生成快照。
    ///
    /// 允许更新布局缓存；不要在这里执行 IO、启动任务或注册监听器。
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot>;

    fn handle_event(
        &mut self,
        _event: &ComponentEvent,
        _host: &mut dyn ComponentHost,
    ) -> ComponentResult<EventResponse> {
        Ok(EventResponse::default())
    }

    fn wants_key_release(&self) -> bool {
        false
    }

    /// 清除组件自身的布局缓存。
    fn invalidate(&mut self) {}

    fn save_state(&self) -> ComponentResult<Option<StateSnapshot>> {
        Ok(None)
    }

    /// 热重载时先恢复状态，再调用 mount。
    ///
    /// 默认不支持恢复状态；支持状态保存的组件需要实现此方法。
    fn restore_state(&mut self, state: &StateSnapshot) -> ComponentResult<()> {
        Err(ComponentError::UnsupportedState {
            version: state.schema_version,
        })
    }

    /// 即使此方法失败，运行时也必须清理 host 注册的资源。
    fn unmount(&mut self, _host: &mut dyn ComponentHost) -> ComponentResult<()> {
        Ok(())
    }
}
