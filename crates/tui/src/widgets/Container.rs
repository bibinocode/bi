//! Container 组件 容器组件，用来组织其他组件
use crate::component::{ChildPlacement, Component, ComponentNode, LayoutSnapshot};
use crate::protocol::{ClipRect, ComponentError, ComponentResult, LayoutContext, Offset};

/// 按子节点顺序纵向排列组件
/// 容器用 available_height: None 计算每个子组件的完整高度。
/// 容器自身的高度受传入的 available_height 限制。
/// 超出容器范围的内容由 clip 裁剪，不改变子组件的完整快照。
#[derive(Debug, Default, Clone, Copy)]
pub struct Container;

impl Container {
    pub const fn new() -> Self {
        Self
    }
}

impl Component for Container {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        // 子组件计算完整内容高度。
        // 每个 ComponentNode 会覆盖 focused，使用自身的焦点状态。
        let child_context = LayoutContext {
            available_height: None,
            ..*context
        };

        let mut placements = Vec::with_capacity(children.len());
        let mut content_height = 0usize;

        for child in children {
            let row = i64::try_from(content_height).map_err(|_| ComponentError::InvalidLayout {
                reason: "Container child offset exceeds i64".into(),
            })?;

            let node = child.layout(&child_context)?;

            // 累加子组件的完整高度；容器可见高度稍后单独限制。
            let next_height = content_height
                .checked_add(node.snapshot.height)
                .ok_or_else(|| ComponentError::InvalidLayout {
                    reason: "Container content height overflow".into(),
                })?;

            // 布局坐标使用 i64，因此完整内容也必须落在该范围内。
            i64::try_from(next_height).map_err(|_| ComponentError::InvalidLayout {
                reason: "Container content height exceeds i64".into(),
            })?;

            placements.push(ChildPlacement {
                offset: Offset { column: 0, row },
                clip: None,
                node,
            });

            content_height = next_height;
        }

        let height = match context.available_height {
            Some(limit) => content_height.min(usize::from(limit)),
            None => content_height,
        };

        // clip 使用父组件坐标，所有子节点共享容器的可见范围。
        let clip = ClipRect {
            column: 0,
            row: 0,
            width: context.width,
            height,
        };

        for placement in &mut placements {
            placement.clip = Some(clip);
        }

        Ok(LayoutSnapshot {
            width: context.width,
            height,
            children: placements,
            ..LayoutSnapshot::default()
        })
    }
}
