use crate::{
    component::{ChildPlacement, Component, ComponentNode, LayoutSnapshot},
    protocol::{ClipRect, ComponentError, ComponentResult, LayoutContext, Offset},
};

/// 浮层尺寸及位置。None 坐标表示在当前可见区域居中。
#[derive(Debug, Clone, Copy)]
pub struct OverlayPlacement {
    pub width: u16,
    pub height: u16,
    pub column: Option<u16>,
    pub row: Option<u16>,
    pub visible: bool,
}
impl OverlayPlacement {
    pub fn centered(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            column: None,
            row: None,
            visible: true,
        }
    }
    pub fn at(mut self, column: u16, row: u16) -> Self {
        self.column = Some(column);
        self.row = Some(row);
        self
    }
}

/// 第一子节点为底层内容，其余子节点按配置顺序叠加，后面的浮层在最上方。
/// 浮层会遮住自身矩形内的底层文字；焦点由 Runtime 管理。
#[derive(Default)]
pub struct Overlay {
    placements: Vec<OverlayPlacement>,
}
impl Overlay {
    pub fn new(placements: impl IntoIterator<Item = OverlayPlacement>) -> Self {
        Self {
            placements: placements.into_iter().collect(),
        }
    }
    pub fn set_visible(&mut self, index: usize, visible: bool) {
        if let Some(placement) = self.placements.get_mut(index) {
            placement.visible = visible;
        }
    }
}
impl Component for Overlay {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        if children.is_empty() || children.len() != self.placements.len() + 1 {
            return Err(ComponentError::InvalidLayout {
                reason: "Overlay requires a base child and one child per placement".into(),
            });
        }
        let base = children[0].layout(context)?;
        let height = context
            .available_height
            .map_or(base.snapshot.height, usize::from);
        let mut result = vec![ChildPlacement {
            offset: Offset::default(),
            clip: Some(ClipRect {
                column: 0,
                row: 0,
                width: context.width,
                height,
            }),
            node: base,
        }];
        for (child, placement) in children[1..].iter_mut().zip(&self.placements) {
            if !placement.visible {
                continue;
            }
            let width = placement.width.min(context.width);
            let child_height = usize::from(placement.height).min(height);
            let column = placement.column.map_or((context.width - width) / 2, |x| {
                x.min(context.width - width)
            });
            let row = placement.row.map_or((height - child_height) / 2, |y| {
                usize::from(y).min(height - child_height)
            });
            let mut node = child.layout(&LayoutContext {
                width,
                available_height: Some(child_height as u16),
                ..*context
            })?;
            // 空白区域也遮住底层；图片与链接的终端合成使用相同区域。
            node.snapshot.height = child_height;
            node.snapshot.width = width;
            node.snapshot.background.get_or_insert_default();
            let offset = Offset {
                column: i32::from(column),
                row: i64::try_from(row).map_err(|_| ComponentError::InvalidLayout {
                    reason: "overlay offset exceeds i64".into(),
                })?,
            };
            result.push(ChildPlacement {
                offset,
                clip: Some(ClipRect {
                    column: offset.column,
                    row: offset.row,
                    width,
                    height: child_height,
                }),
                node,
            });
        }
        Ok(LayoutSnapshot {
            width: context.width,
            height,
            children: result,
            ..LayoutSnapshot::default()
        })
    }
}
