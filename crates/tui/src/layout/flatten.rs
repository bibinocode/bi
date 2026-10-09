use crate::component::{LayoutNode, LayoutSnapshot};
use crate::protocol::{ClipRect, ComponentHandle, ComponentResult, Offset};

use super::{intersect_rect, resolve_child};

/// 展开后的可见布局节点。
#[derive(Debug)]
pub struct FlatNode<'a> {
    pub handle: ComponentHandle,

    /// 节点原点，使用全局坐标。
    pub offset: Offset,

    /// 最终可见范围，使用全局坐标。
    pub clip: ClipRect,

    /// 父节点在展开列表中的索引。
    pub parent: Option<usize>,

    pub snapshot: &'a LayoutSnapshot,
}

/// 按绘制顺序展开布局树。
///
/// 先父节点，再按 children 的顺序展开子节点。
/// viewport 与 root_offset 必须使用同一个全局坐标系。
pub fn flatten_layout<'a>(
    root: &'a LayoutNode,
    root_offset: Offset,
    viewport: ClipRect,
) -> ComponentResult<Vec<FlatNode<'a>>> {
    let bounds = ClipRect {
        column: 0,
        row: 0,
        width: root.snapshot.width,
        height: root.snapshot.height,
    }
    .translated(root_offset)?;

    let clip = intersect_rect(viewport, bounds)?;

    let mut nodes = Vec::new();

    visit(root, root_offset, clip, None, &mut nodes)?;

    Ok(nodes)
}

/// 返回指定位置上绘制顺序最靠后的可见节点。
///
/// 这里只判断几何范围，不判断组件是否处理鼠标事件。
pub fn hit_test<'a, 'tree>(
    nodes: &'a [FlatNode<'tree>],
    column: i32,
    row: i64,
) -> Option<&'a FlatNode<'tree>> {
    nodes.iter().rev().find(|node| {
        let clip = node.clip;

        if clip.is_empty() {
            return false;
        }

        let relative_column = i64::from(column) - i64::from(clip.column);

        let Some(relative_row) = row.checked_sub(clip.row) else {
            return false;
        };

        let Ok(relative_row) = usize::try_from(relative_row) else {
            return false;
        };

        relative_column >= 0
            && relative_column < i64::from(clip.width)
            && relative_row < clip.height
    })
}

fn visit<'a>(
    node: &'a LayoutNode,
    offset: Offset,
    clip: ClipRect,
    parent: Option<usize>,
    nodes: &mut Vec<FlatNode<'a>>,
) -> ComponentResult<()> {
    if clip.is_empty() {
        return Ok(());
    }

    let index = nodes.len();

    nodes.push(FlatNode {
        handle: node.handle,
        offset,
        clip,
        parent,
        snapshot: &node.snapshot,
    });

    for placement in &node.snapshot.children {
        let resolved = resolve_child(offset, clip, placement)?;

        visit(
            &placement.node,
            resolved.offset,
            resolved.clip,
            Some(index),
            nodes,
        )?;
    }

    Ok(())
}
