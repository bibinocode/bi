use crate::component::ChildPlacement;
use crate::protocol::{ClipRect, ComponentError, ComponentResult, Offset};

/// 子组件转换到全局坐标后的布局信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedPlacement {
    pub offset: Offset,
    pub clip: ClipRect,
}

/// 求两个矩形的交集。
///
/// 两个矩形必须使用同一个坐标系。
/// 边界采用半开区间：包含左上边界，不包含右下边界。
pub fn intersect_rect(first: ClipRect, second: ClipRect) -> ComponentResult<ClipRect> {
    let (first_right, first_bottom) = rect_end(first)?;
    let (second_right, second_bottom) = rect_end(second)?;

    let column = first.column.max(second.column);
    let row = first.row.max(second.row);

    let right = first_right.min(second_right);
    let bottom = first_bottom.min(second_bottom);

    if right <= i64::from(column) || bottom <= row {
        return Ok(ClipRect {
            column,
            row,
            width: 0,
            height: 0,
        });
    }

    let width =
        u16::try_from(right - i64::from(column)).map_err(|_| ComponentError::InvalidLayout {
            reason: "Clip intersection width exceeds u16".into(),
        })?;

    let height = usize::try_from(bottom - row).map_err(|_| ComponentError::InvalidLayout {
        reason: "Clip intersection height exceeds usize".into(),
    })?;

    Ok(ClipRect {
        column,
        row,
        width,
        height,
    })
}

/// 将子组件的位置和裁剪范围转换到全局坐标。
///
/// parent_offset 和 parent_clip 都使用全局坐标。
pub fn resolve_child(
    parent_offset: Offset,
    parent_clip: ClipRect,
    placement: &ChildPlacement,
) -> ComponentResult<ResolvedPlacement> {
    let offset = parent_offset.checked_add(placement.offset)?;

    // 子组件自身范围，从子组件坐标转换到全局坐标。
    let bounds = ClipRect {
        column: 0,
        row: 0,
        width: placement.node.snapshot.width,
        height: placement.node.snapshot.height,
    }
    .translated(offset)?;

    let mut clip = intersect_rect(parent_clip, bounds)?;

    if let Some(local_clip) = placement.clip {
        // ChildPlacement.clip 使用父组件坐标。
        let global_clip = local_clip.translated(parent_offset)?;
        clip = intersect_rect(clip, global_clip)?;
    }

    Ok(ResolvedPlacement { offset, clip })
}

/// 计算右边界和下边界，并检查坐标溢出。
fn rect_end(rect: ClipRect) -> ComponentResult<(i64, i64)> {
    // column 是 i32，width 是 u16；相加使用 i64。
    let right = i64::from(rect.column) + i64::from(rect.width);

    let height = i64::try_from(rect.height).map_err(|_| ComponentError::InvalidLayout {
        reason: "Clip height exceeds i64".into(),
    })?;

    let bottom = rect
        .row
        .checked_add(height)
        .ok_or_else(|| ComponentError::InvalidLayout {
            reason: "Clip bottom coordinate overflow".into(),
        })?;

    Ok((right, bottom))
}
