//! 文本差异绘制后的 OSC 8 / 图片协议输出。所有控制序列由库生成。
use crate::{
    component::LayoutNode,
    layout::flatten_layout,
    protocol::{
        Capabilities, ClipRect, ComponentError, ComponentResult, ImagePlacement, ImageProtocol,
        Offset,
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};
use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier},
};
use std::{
    io::{self, Cursor, Write},
    sync::atomic::{AtomicU32, Ordering},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
pub struct TerminalOutput {
    image_ids: Vec<u32>,
    had_iterm_images: bool,
    linked_cells: std::collections::HashSet<(u16, u16)>,
}
impl TerminalOutput {
    /// iTerm2 没有按 ID 删除接口，下一帧需要重新绘制文本区域。
    pub fn needs_clear(&self) -> bool {
        self.had_iterm_images
    }
    pub fn clear(&mut self, writer: &mut impl Write) -> io::Result<()> {
        for id in &self.image_ids {
            write!(writer, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\")?;
        }
        self.image_ids.clear();
        self.had_iterm_images = false;
        Ok(())
    }
    pub fn write_frame(
        &mut self,
        writer: &mut impl Write,
        root: &LayoutNode,
        offset: Offset,
        viewport: ClipRect,
        buffer: &Buffer,
        capabilities: Capabilities,
    ) -> ComponentResult<()> {
        crate::layout::validate_layout(root)?;
        if viewport.column < 0
            || viewport.row < 0
            || i64::from(viewport.column) + i64::from(viewport.width) > i64::from(u16::MAX) + 1
            || viewport.row.saturating_add(viewport.height as i64) > i64::from(u16::MAX) + 1
        {
            return Err(invalid(
                "terminal output viewport exceeds terminal coordinate range",
            ));
        }
        let mut bytes = Vec::new();
        // 先完整编码再输出，内容失败不会留下半截控制序列。
        let nodes = flatten_layout(root, offset, viewport)?;
        let w = usize::from(viewport.width);
        let h = viewport.height;
        let len = w
            .checked_mul(h)
            .ok_or_else(|| invalid("external frame size overflow"))?;
        let mut links: Vec<Option<String>> = vec![None; len];
        let mut owners = vec![None; len];
        let mut images: Vec<(&ImagePlacement, i32, i64)> = Vec::new();
        let index = |x: i32, y: i64| -> Option<usize> {
            let x = x.checked_sub(viewport.column)?;
            let y = y.checked_sub(viewport.row)?;
            if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
                None
            } else {
                Some(y as usize * w + x as usize)
            }
        };
        for node in nodes {
            if node.snapshot.background.is_some() {
                for row in 0..node.clip.height {
                    for col in 0..node.clip.width {
                        if let Some(i) = index(
                            node.clip.column + i32::from(col),
                            node.clip.row + row as i64,
                        ) {
                            links[i] = None;
                            owners[i] = None;
                        }
                    }
                }
            }
            for image in &node.snapshot.images {
                let x = node.offset.column + i32::from(image.position.column);
                let y = node.offset.row + image.position.row as i64;
                let id = images.len();
                images.push((image, x, y));
                for row in 0..image.size.height {
                    for col in 0..image.size.width {
                        let px = x + i32::from(col);
                        let py = y + i64::from(row);
                        if px >= node.clip.column
                            && px < node.clip.column + i32::from(node.clip.width)
                            && py >= node.clip.row
                            && py < node.clip.row + node.clip.height as i64
                            && let Some(i) = index(px, py)
                        {
                            owners[i] = Some(id);
                            links[i] = None;
                        }
                    }
                }
            }
            for (row, line) in node.snapshot.lines.iter().enumerate() {
                let y = node.offset.row + row as i64;
                if y < node.clip.row || y >= node.clip.row + node.clip.height as i64 {
                    continue;
                }
                let text: String = line.spans.iter().map(|s| s.text.as_str()).collect();
                let mut ends = Vec::new();
                let mut end = 0;
                for span in &line.spans {
                    end += span.text.len();
                    ends.push(end);
                }
                let mut span_index = 0;
                let mut column = 0;
                for (byte, grapheme) in text.grapheme_indices(true) {
                    while ends[span_index] <= byte {
                        span_index += 1;
                    }
                    let size = UnicodeWidthStr::width(grapheme);
                    let x = node.offset.column + column as i32;
                    column += size;
                    if x < node.clip.column
                        || x + size as i32 > node.clip.column + i32::from(node.clip.width)
                    {
                        continue;
                    }
                    for cell in 0..size {
                        if let Some(i) = index(x + cell as i32, y) {
                            owners[i] = None;
                            links[i] = line.spans[span_index].hyperlink.clone();
                        }
                    }
                }
            }
        }
        bytes.extend_from_slice(b"\x1b7");
        for id in &self.image_ids {
            write!(bytes, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\").unwrap();
        }
        let mut new_ids = Vec::new();
        if let Some(protocol) = capabilities.images {
            for (image_id, (placement, x, y)) in images.iter().enumerate() {
                // 合并相同水平区间的连续行，再按原图比例裁剪，避免穿透上层浮层。
                let mut rectangles: Vec<(usize, usize, usize, usize)> = Vec::new();
                for row in 0..h {
                    let mut col = 0;
                    while col < w {
                        if owners[row * w + col] != Some(image_id) {
                            col += 1;
                            continue;
                        }
                        let start = col;
                        while col < w && owners[row * w + col] == Some(image_id) {
                            col += 1;
                        }
                        if let Some(rect) = rectangles
                            .iter_mut()
                            .find(|r| r.0 == start && r.2 == col - start && r.1 + r.3 == row)
                        {
                            rect.3 += 1;
                        } else {
                            rectangles.push((start, row, col - start, 1));
                        }
                    }
                }
                if rectangles.is_empty() {
                    continue;
                }
                let decoded = image::load_from_memory(&placement.data)
                    .map_err(|e| invalid(&e.to_string()))?;
                for (col, row, columns, rows) in rectangles {
                    let left = viewport.column + col as i32;
                    let top = viewport.row + row as i64;
                    let px = (u64::try_from(left - *x).unwrap() * u64::from(decoded.width())
                        / u64::from(placement.size.width)) as u32;
                    let py = (u64::try_from(top - *y).unwrap() * u64::from(decoded.height())
                        / u64::from(placement.size.height)) as u32;
                    let right = (((left - *x) as u64 + columns as u64) * u64::from(decoded.width())
                        / u64::from(placement.size.width)) as u32;
                    let bottom = (((top - *y) as u64 + rows as u64) * u64::from(decoded.height())
                        / u64::from(placement.size.height)) as u32;
                    if right <= px || bottom <= py {
                        continue;
                    }
                    let crop = decoded.crop_imm(px, py, right - px, bottom - py);
                    let mut png = Cursor::new(Vec::new());
                    crop.write_to(&mut png, image::ImageFormat::Png)
                        .map_err(|e| invalid(&e.to_string()))?;
                    let encoded = STANDARD.encode(png.into_inner());
                    write!(bytes, "\x1b[{};{}H", top + 1, left + 1).unwrap();
                    match protocol {
                        ImageProtocol::Kitty => {
                            static NEXT_ID: AtomicU32 = AtomicU32::new(0x40000000);
                            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                            new_ids.push(id);
                            let chunks: Vec<_> = encoded.as_bytes().chunks(4096).collect();
                            for (i, chunk) in chunks.iter().enumerate() {
                                if i == 0 {
                                    write!(bytes,"\x1b_Ga=T,f=100,t=d,i={id},c={columns},r={rows},q=2,C=1,m={};",u8::from(i+1<chunks.len())).unwrap();
                                } else {
                                    write!(bytes, "\x1b_Gm={};", u8::from(i + 1 < chunks.len()))
                                        .unwrap();
                                }
                                bytes.extend_from_slice(chunk);
                                bytes.extend_from_slice(b"\x1b\\");
                            }
                        }
                        ImageProtocol::Iterm2 => {
                            write!(bytes,"\x1b]1337;File=inline=1;width={columns};height={rows};preserveAspectRatio=0:{encoded}\x07").unwrap();
                        }
                    }
                }
            }
        }
        let mut new_links = std::collections::HashSet::new();
        if capabilities.hyperlinks || !self.linked_cells.is_empty() {
            for row in 0..h {
                for col in 0..w {
                    let i = row * w + col;
                    let x = viewport.column + col as i32;
                    let y = viewport.row + row as i64;
                    let url = if capabilities.hyperlinks {
                        links[i].as_deref().unwrap_or("")
                    } else {
                        ""
                    };
                    if owners[i].is_some()
                        || (url.is_empty() && !self.linked_cells.contains(&(x as u16, y as u16)))
                    {
                        continue;
                    }
                    if !url.is_empty() {
                        new_links.insert((x as u16, y as u16));
                    }
                    if url.chars().any(char::is_control) {
                        return Err(invalid("hyperlink contains control character"));
                    }
                    let x = viewport.column + col as i32;
                    let y = viewport.row + row as i64;
                    let Some(cell) = buffer.cell((x as u16, y as u16)) else {
                        continue;
                    };
                    // 宽字素后续格不再打印，避免覆盖前一格。
                    if col > 0
                        && buffer
                            .cell(((x - 1) as u16, y as u16))
                            .is_some_and(|c| UnicodeWidthStr::width(c.symbol()) > 1)
                    {
                        continue;
                    }
                    write!(bytes, "\x1b[{};{}H\x1b[0m", y + 1, x + 1).unwrap();
                    sgr_color(&mut bytes, cell.fg, false);
                    sgr_color(&mut bytes, cell.bg, true);
                    for (modifier, code) in [
                        (Modifier::BOLD, 1),
                        (Modifier::DIM, 2),
                        (Modifier::ITALIC, 3),
                        (Modifier::UNDERLINED, 4),
                        (Modifier::REVERSED, 7),
                        (Modifier::CROSSED_OUT, 9),
                    ] {
                        if cell.modifier.contains(modifier) {
                            write!(bytes, "\x1b[{code}m").unwrap();
                        }
                    }
                    write!(bytes, "\x1b]8;;{url}\x1b\\{}\x1b]8;;\x1b\\", cell.symbol()).unwrap();
                }
            }
        }
        bytes.extend_from_slice(b"\x1b[0m\x1b8");
        self.image_ids = new_ids;
        self.linked_cells = new_links;
        writer
            .write_all(&bytes)
            .and_then(|_| writer.flush())
            .map_err(|e| ComponentError::OperationFailed {
                message: e.to_string(),
            })?;
        self.had_iterm_images =
            capabilities.images == Some(ImageProtocol::Iterm2) && !images.is_empty();
        Ok(())
    }
}
fn invalid(reason: &str) -> ComponentError {
    ComponentError::InvalidContent {
        reason: reason.into(),
    }
}
fn sgr_color(bytes: &mut Vec<u8>, color: Color, background: bool) {
    let prefix = if background { 48 } else { 38 };
    match color {
        Color::Rgb(r, g, b) => write!(bytes, "\x1b[{prefix};2;{r};{g};{b}m").unwrap(),
        Color::Indexed(i) => write!(bytes, "\x1b[{prefix};5;{i}m").unwrap(),
        Color::Reset => {}
        other => {
            let index = match other {
                Color::Black => 0,
                Color::Red => 1,
                Color::Green => 2,
                Color::Yellow => 3,
                Color::Blue => 4,
                Color::Magenta => 5,
                Color::Cyan => 6,
                Color::Gray => 7,
                Color::DarkGray => 8,
                Color::LightRed => 9,
                Color::LightGreen => 10,
                Color::LightYellow => 11,
                Color::LightBlue => 12,
                Color::LightMagenta => 13,
                Color::LightCyan => 14,
                Color::White => 15,
                _ => 0,
            };
            write!(bytes, "\x1b[{prefix};5;{index}m").unwrap();
        }
    }
}
