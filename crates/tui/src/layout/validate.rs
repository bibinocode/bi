//! 布局快照检验

use std::collections::HashSet;

use crate::component::LayoutNode;
use crate::protocol::{ClipRect, ComponentError, ComponentId, ComponentResult, Offset};
use crate::utils::text::display_width;

use super::intersect_rect;

/// 检查整棵布局树，包括不可见节点。
pub fn validate_layout(root: &LayoutNode) -> ComponentResult<()> {
    let mut ids = HashSet::new();

    validate_node(root, Offset::default(), &mut ids)
}

fn validate_node(
    node: &LayoutNode,
    offset: Offset,
    ids: &mut HashSet<ComponentId>,
) -> ComponentResult<()> {
    if !ids.insert(node.handle.id) {
        return Err(ComponentError::InvalidLayout {
            reason: format!("duplicate component ID: {:?}", node.handle.id,),
        });
    }

    let snapshot = &node.snapshot;

    if snapshot.lines.len() > snapshot.height {
        return Err(ComponentError::InvalidLayout {
            reason: "text line count exceeds snapshot height".into(),
        });
    }

    // 即使节点不可见，也检查其全局坐标范围。
    let bounds = ClipRect {
        column: 0,
        row: 0,
        width: snapshot.width,
        height: snapshot.height,
    }
    .translated(offset)?;

    intersect_rect(bounds, bounds)?;

    for (row, line) in snapshot.lines.iter().enumerate() {
        let mut text = String::new();

        for span in &line.spans {
            text.push_str(&span.text);

            if let Some(link) = &span.hyperlink
                && link.chars().any(char::is_control)
            {
                return Err(ComponentError::InvalidContent {
                    reason: "hyperlink contains a control character".into(),
                });
            }
        }

        // 同时拒绝换行、制表符和 ANSI 控制字符。
        let width = display_width(&text)?;

        if width > usize::from(snapshot.width) {
            return Err(ComponentError::InvalidLayout {
                reason: format!(
                    "text row {row} has width {width}, \
                     exceeding snapshot width {}",
                    snapshot.width,
                ),
            });
        }
    }

    if let Some(cursor) = snapshot.cursor
        && (cursor.position.column >= snapshot.width || cursor.position.row >= snapshot.height)
    {
        return Err(ComponentError::InvalidLayout {
            reason: "cursor is outside snapshot bounds".into(),
        });
    }

    for image in &snapshot.images {
        if image.size.is_empty() {
            return Err(ComponentError::InvalidLayout {
                reason: "image placement has an empty size".into(),
            });
        }

        let right = u32::from(image.position.column) + u32::from(image.size.width);

        let bottom = image
            .position
            .row
            .checked_add(usize::from(image.size.height))
            .ok_or_else(|| ComponentError::InvalidLayout {
                reason: "image bottom coordinate overflow".into(),
            })?;

        if right > u32::from(snapshot.width) || bottom > snapshot.height {
            return Err(ComponentError::InvalidLayout {
                reason: "image placement exceeds snapshot bounds".into(),
            });
        }
    }

    for placement in &snapshot.children {
        let child_offset = offset.checked_add(placement.offset)?;

        if let Some(clip) = placement.clip {
            // clip 使用当前父节点坐标。
            let global_clip = clip.translated(offset)?;
            intersect_rect(global_clip, global_clip)?;
        }

        validate_node(&placement.node, child_offset, ids)?;
    }

    Ok(())
}
