use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::protocol::{ComponentError, ComponentResult};

/// 计算单行纯文本占用的终端列数。
///
/// 不接受换行、制表符或 ANSI 控制序列。
pub fn display_width(text: &str) -> ComponentResult<usize> {
    validate_text(text, false)?;

    Ok(UnicodeWidthStr::width(text))
}

/// 将纯文本按终端列宽硬换行。
pub fn wrap_text(text: &str, width: u16) -> ComponentResult<Vec<String>> {
    let normalized = text.replace("\r\n", "\n");

    validate_text(&normalized, true)?;

    if width == 0 {
        return Ok(Vec::new());
    }

    let limit = usize::from(width);
    let mut lines = Vec::new();

    // split 保留空行以及末尾换行产生的最后一个空行。
    for source_line in normalized.split('\n') {
        let mut line = String::new();
        let mut line_width = 0usize;

        for grapheme in source_line.graphemes(true) {
            let grapheme_width = UnicodeWidthStr::width(grapheme);

            let (piece, piece_width) = if grapheme_width > limit {
                ("\u{FFFD}", 1)
            } else {
                (grapheme, grapheme_width)
            };

            if line_width + piece_width > limit {
                lines.push(std::mem::take(&mut line));
                line_width = 0;
            }

            line.push_str(piece);
            line_width += piece_width;
        }

        lines.push(line);
    }

    Ok(lines)
}

pub(crate) fn validate_text(text: &str, allow_newlines: bool) -> ComponentResult<()> {
    for character in text.chars() {
        if allow_newlines && character == '\n' {
            continue;
        }

        if character.is_control() {
            return Err(ComponentError::InvalidContent {
                reason: format!(
                    "unsupported control character U+{:04X}",
                    u32::from(character),
                ),
            });
        }
    }

    Ok(())
}

/// 按字素簇截取单行前缀，不拆开中文、组合字符或 emoji。
pub fn prefix_by_width(text: &str, width: u16) -> ComponentResult<String> {
    display_width(text)?;
    let mut used = 0;
    Ok(text
        .graphemes(true)
        .take_while(|grapheme| {
            used += UnicodeWidthStr::width(*grapheme);
            used <= usize::from(width)
        })
        .collect())
}

/// 超宽时添加省略标记；标记本身过宽时也按字素簇截断。
pub fn truncate_text(text: &str, width: u16, ellipsis: &str) -> ComponentResult<String> {
    let measured = display_width(text)?;
    display_width(ellipsis)?;
    if measured <= usize::from(width) {
        return Ok(text.to_owned());
    }
    let suffix = prefix_by_width(ellipsis, width)?;
    let remaining = width - display_width(&suffix)? as u16;
    Ok(prefix_by_width(text, remaining)? + &suffix)
}
