use crate::component::{Component, ComponentNode, LayoutSnapshot};
use crate::protocol::{ComponentError, ComponentResult, LayoutContext};

/// Spacer 保存的是期望高度，布局返回的是实际分配高度。
/// 例如期望占用 3 行，但 available_height 只有 2 行，就返回 2 行；没有高度限制时返回 3 行
/// 占用指定数量的空白行。
#[derive(Debug, Clone)]
pub struct Spacer {
    height: usize,
}

impl Spacer {
    pub const fn new(height: usize) -> Self {
        Self { height }
    }

    pub const fn height(&self) -> usize {
        self.height
    }

    /// 修改期望高度。
    ///
    /// 运行时接入后，调用方还需要请求重绘。
    pub fn set_height(&mut self, height: usize) {
        self.height = height;
    }
}

impl Default for Spacer {
    fn default() -> Self {
        Self::new(1)
    }
}

impl Component for Spacer {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        if !children.is_empty() {
            return Err(ComponentError::InvalidLayout {
                reason: "Spacer does not accept child components".into(),
            });
        }

        let height = match context.available_height {
            Some(limit) => self.height.min(usize::from(limit)),
            None => self.height,
        };

        Ok(LayoutSnapshot {
            width: context.width,
            height,
            ..LayoutSnapshot::default()
        })
    }
}
