use crate::component::{Component, ComponentNode, LayoutSnapshot};
use crate::protocol::{ClipRect, ComponentError, ComponentResult, LayoutContext, Offset, Style};

use super::Container;

/// 内边距，单位为终端单元格。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Padding {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl Padding {
    pub const fn all(value: u16) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn symmetric(vertical: u16, horizontal: u16) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
}

/// 带内边距和背景样式的纵向容器。
#[derive(Debug, Default, Clone)]
pub struct BoxContainer {
    padding: Padding,
    background: Option<Style>,
}

impl BoxContainer {
    pub const fn new(padding: Padding) -> Self {
        Self {
            padding,
            background: None,
        }
    }

    pub fn with_background(mut self, style: Style) -> Self {
        self.background = Some(style);
        self
    }

    pub fn set_padding(&mut self, padding: Padding) {
        self.padding = padding;
    }

    pub fn set_background(&mut self, background: Option<Style>) {
        self.background = background;
    }
}

impl Component for BoxContainer {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        let inner_width = context
            .width
            .saturating_sub(self.padding.left)
            .saturating_sub(self.padding.right);

        // 复用 Container 计算完整内容布局。
        let inner_context = LayoutContext {
            width: inner_width,
            available_height: None,
            ..*context
        };

        let mut container = Container::new();
        let inner = container.layout(&inner_context, children)?;

        let natural_height = inner
            .height
            .checked_add(usize::from(self.padding.top))
            .and_then(|height| height.checked_add(usize::from(self.padding.bottom)))
            .ok_or_else(|| ComponentError::InvalidLayout {
                reason: "BoxContainer height overflow".into(),
            })?;

        i64::try_from(natural_height).map_err(|_| ComponentError::InvalidLayout {
            reason: "BoxContainer height exceeds i64".into(),
        })?;

        let height = match context.available_height {
            Some(limit) => natural_height.min(usize::from(limit)),
            None => natural_height,
        };

        // 高度不足时，内边距会压缩内容可见区域，最小为 0。
        let inner_height = height
            .saturating_sub(usize::from(self.padding.top))
            .saturating_sub(usize::from(self.padding.bottom));

        let offset = Offset {
            column: i32::from(self.padding.left),
            row: i64::from(self.padding.top),
        };

        let clip = ClipRect {
            column: offset.column,
            row: offset.row,
            width: inner_width,
            height: inner_height,
        };

        let mut placements = inner.children;

        for placement in &mut placements {
            placement.offset = placement.offset.checked_add(offset)?;

            // Container 生成的裁剪范围恰好是其整个内容区域；
            // 这里改为 BoxContainer 的内部可见区域。
            placement.clip = Some(clip);
        }

        Ok(LayoutSnapshot {
            width: context.width,
            height,
            background: self.background,
            children: placements,
            ..LayoutSnapshot::default()
        })
    }
}
