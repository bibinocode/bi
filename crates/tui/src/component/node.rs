use crate::protocol::{ComponentError, ComponentHandle, ComponentResult, LayoutContext};

use super::{Component, LayoutNode};

/// 跨帧存在的组件实例节点。
///
/// 保存组件对象和子组件状态。
/// layout() 生成的 LayoutNode 则只保存本次布局结果。
pub struct ComponentNode {
    pub handle: ComponentHandle,
    pub component: Box<dyn Component>,
    pub children: Vec<ComponentNode>,

    focused: bool,
}

impl ComponentNode {
    pub fn new(handle: ComponentHandle, component: Box<dyn Component>) -> Self {
        Self {
            handle,
            component,
            children: Vec::new(),
            focused: false,
        }
    }

    /// 仅添加到实例树。
    ///
    /// 挂载、注册和重绘请求由宿主另行执行。
    pub fn add_child(&mut self, child: ComponentNode) {
        self.children.push(child);
    }

    /// 由宿主根据焦点管理结果设置。
    ///
    /// 本方法不派发 FocusChanged，也不自动请求重绘。
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn layout(&mut self, context: &LayoutContext) -> ComponentResult<LayoutNode> {
        let mut local_context = *context;
        local_context.focused = self.focused;

        let snapshot = self.component.layout(&local_context, &mut self.children)?;

        // 先校验最基本的尺寸契约。
        // 文本宽度、光标、图片等详细校验后续统一实现。
        if snapshot.width > local_context.width {
            return Err(ComponentError::InvalidLayout {
                reason: format!(
                    "component width {} exceeds available width {}",
                    snapshot.width, local_context.width,
                ),
            });
        }

        if let Some(limit) = local_context.available_height
            && snapshot.height > usize::from(limit)
        {
            return Err(ComponentError::InvalidLayout {
                reason: format!(
                    "component height {} exceeds available height {}",
                    snapshot.height, limit,
                ),
            });
        }

        if snapshot.lines.len() > snapshot.height {
            return Err(ComponentError::InvalidLayout {
                reason: "text line count exceeds component height".into(),
            });
        }

        Ok(LayoutNode {
            handle: self.handle,
            snapshot,
        })
    }

    /// 清除整个子树的布局缓存，保留组件状态。
    pub fn invalidate_subtree(&mut self) {
        self.component.invalidate();

        for child in &mut self.children {
            child.invalidate_subtree();
        }
    }
}
