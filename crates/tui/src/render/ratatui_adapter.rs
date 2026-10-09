use ratatui::{
    buffer::Buffer,
    style::{Color as RatatuiColor, Modifier, Style as RatatuiStyle},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::component::LayoutNode;
use crate::layout::flatten_layout;
use crate::protocol::{ClipRect, Color, ComponentError, ComponentResult, Offset, Style};

/// 将布局绘制到整个 Buffer。
///
/// 坐标直接对应 Buffer 坐标。
/// 调用方需要在终端尺寸变化后重新计算布局。
pub fn paint_layout(
    root: &LayoutNode,
    root_offset: Offset,
    buffer: &mut Buffer,
) -> ComponentResult<()> {
    crate::layout::validate_layout(root)?;
    let area = buffer.area;

    let viewport = ClipRect {
        column: i32::from(area.x),
        row: i64::from(area.y),
        width: area.width,
        height: usize::from(area.height),
    };

    let nodes = flatten_layout(root, root_offset, viewport)?;

    for node in &nodes {
        if !node.snapshot.images.is_empty() {
            return Err(ComponentError::OperationFailed {
                message: "image rendering is not implemented".into(),
            });
        }
    }

    // 本函数拥有整个 Buffer 的绘制范围。
    // 先清除旧内容，保证 Spacer 和空白区域不会残留文字。
    buffer.reset();

    for node in nodes {
        let clip = node.clip;

        if let Some(background) = node.snapshot.background {
            let style = to_ratatui_style(background);

            for row in 0..clip.height {
                for column in 0..usize::from(clip.width) {
                    let x = (i64::from(clip.column) + column as i64) as u16;
                    let y = (clip.row + row as i64) as u16;

                    if let Some(cell) = buffer.cell_mut((x, y)) {
                        cell.reset();
                        cell.set_style(style);
                    }
                }
            }
        }

        // flatten_layout 保证 clip 位于节点自身范围内。
        let first_row = (clip.row - node.offset.row) as usize;

        for (local_row, line) in node
            .snapshot
            .lines
            .iter()
            .enumerate()
            .skip(first_row)
            .take(clip.height)
        {
            let y = (node.offset.row + local_row as i64) as u16;

            // 先拼接，再划分字素簇，避免在 Span 边界拆开字符。
            let mut text = String::new();
            let mut styles = Vec::with_capacity(line.spans.len());

            for span in &line.spans {
                text.push_str(&span.text);
                styles.push((text.len(), span.style));
            }

            let mut style_index = 0usize;
            let mut local_column = 0usize;

            for (byte_index, grapheme) in text.grapheme_indices(true) {
                while styles[style_index].0 <= byte_index {
                    style_index += 1;
                }

                let width = UnicodeWidthStr::width(grapheme);

                let left = i64::from(node.offset.column) + local_column as i64;
                let right = left + width as i64;

                local_column += width;

                let clip_left = i64::from(clip.column);
                let clip_right = clip_left + i64::from(clip.width);

                // 零宽字素不单独占格。
                // 被裁掉一部分的宽字素也不绘制，避免越界。
                if width == 0 || left < clip_left || right > clip_right {
                    continue;
                }

                // 一个字素簇使用其起始字节所属 Span 的样式。
                let style = to_ratatui_style(styles[style_index].1);

                for column in 0..width {
                    let x = (left + column as i64) as u16;

                    if let Some(cell) = buffer.cell_mut((x, y)) {
                        cell.reset();
                        cell.set_style(style);

                        if column == 0 {
                            cell.set_symbol(grapheme);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// 只返回焦点组件在当前视口内的光标位置；被裁剪时隐藏光标。
pub fn layout_cursor(
    root: &LayoutNode,
    focused: Option<crate::protocol::ComponentHandle>,
    offset: Offset,
    viewport: ClipRect,
) -> ComponentResult<Option<(u16, u16)>> {
    for node in flatten_layout(root, offset, viewport)? {
        if Some(node.handle) != focused {
            continue;
        }
        let Some(cursor) = node.snapshot.cursor else {
            return Ok(None);
        };
        let column = i64::from(node.offset.column) + i64::from(cursor.position.column);
        let row = node
            .offset
            .row
            .checked_add(i64::try_from(cursor.position.row).map_err(|_| {
                ComponentError::InvalidLayout {
                    reason: "cursor row exceeds i64".into(),
                }
            })?)
            .ok_or_else(|| ComponentError::InvalidLayout {
                reason: "cursor row overflow".into(),
            })?;
        if column >= i64::from(node.clip.column)
            && column < i64::from(node.clip.column) + i64::from(node.clip.width)
            && row >= node.clip.row
            && row < node.clip.row + node.clip.height as i64
        {
            return Ok(u16::try_from(column).ok().zip(u16::try_from(row).ok()));
        }
    }
    Ok(None)
}

fn to_ratatui_style(style: Style) -> RatatuiStyle {
    let mut modifiers = Modifier::empty();

    for (enabled, modifier) in [
        (style.bold, Modifier::BOLD),
        (style.dim, Modifier::DIM),
        (style.italic, Modifier::ITALIC),
        (style.underline, Modifier::UNDERLINED),
        (style.reversed, Modifier::REVERSED),
        (style.crossed_out, Modifier::CROSSED_OUT),
    ] {
        if enabled {
            modifiers.insert(modifier);
        }
    }

    // 协议中的样式是完整值，不是对旧样式的增量修改。
    RatatuiStyle::default()
        .fg(to_ratatui_color(style.foreground))
        .bg(to_ratatui_color(style.background))
        .remove_modifier(Modifier::all())
        .add_modifier(modifiers)
}

fn to_ratatui_color(color: Option<Color>) -> RatatuiColor {
    match color {
        None => RatatuiColor::Reset,
        Some(Color::Indexed(index)) => RatatuiColor::Indexed(index),
        Some(Color::Rgb(red, green, blue)) => RatatuiColor::Rgb(red, green, blue),
    }
}
