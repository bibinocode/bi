//! 对结构化文本按字素簇折行，保留样式和链接；不接受 ANSI 文本。
use crate::protocol::{ComponentResult, Line, Span};
use crate::utils::text::display_width;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub fn wrap_lines(lines: &[Line], width: u16) -> ComponentResult<Vec<Line>> {
    if width == 0 {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for line in lines {
        let text: String = line.spans.iter().map(|s| s.text.as_str()).collect();
        display_width(&text)?;
        let mut offsets = Vec::new();
        let mut end = 0;
        for span in &line.spans {
            end += span.text.len();
            offsets.push(end);
        }
        let mut current = Line::default();
        let mut used = 0;
        let mut index = 0;
        for (byte, grapheme) in text.grapheme_indices(true) {
            while offsets[index] <= byte {
                index += 1;
            }
            let mut size = UnicodeWidthStr::width(grapheme);
            let value = if size > usize::from(width) {
                size = 1;
                "�"
            } else {
                grapheme
            };
            if used + size > usize::from(width) {
                result.push(std::mem::take(&mut current));
                used = 0;
            }
            let source = &line.spans[index];
            if let Some(last) = current.spans.last_mut()
                && last.style == source.style
                && last.hyperlink == source.hyperlink
            {
                last.text.push_str(value);
            } else {
                current.spans.push(Span {
                    text: value.into(),
                    style: source.style,
                    hyperlink: source.hyperlink.clone(),
                });
            }
            used += size;
        }
        result.push(current);
    }
    Ok(result)
}
