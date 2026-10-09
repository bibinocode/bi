use crate::{
    component::{ChildPlacement, Component, ComponentNode, LayoutNode, LayoutSnapshot},
    protocol::{
        ClipRect, ComponentError, ComponentEvent, ComponentHost, ComponentResult, EventResponse,
        Key, KeyKind, LayoutContext, Line, Offset, Span,
    },
    utils::text::truncate_text,
};
use unicode_segmentation::UnicodeSegmentation;

/// 可搜索纵向文档。Ctrl+F 输入查询，Enter/Shift+Enter 跳转，Escape 关闭搜索。
/// 匹配区分大小写且不跨布局行；查询定位到对应文档行。
pub struct SearchView {
    height: u16,
    query: String,
    searching: bool,
    matches: Vec<usize>,
    selected: usize,
    scroll: usize,
    viewport: usize,
    content: usize,
}
impl SearchView {
    pub fn new(height: u16) -> Self {
        Self {
            height,
            query: String::new(),
            searching: false,
            matches: Vec::new(),
            selected: 0,
            scroll: 0,
            viewport: 0,
            content: 0,
        }
    }
    pub fn set_query(&mut self, query: &str) -> ComponentResult<()> {
        crate::utils::text::display_width(query)?;
        self.query = query.into();
        self.selected = 0;
        Ok(())
    }
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }
    fn visit(node: &mut LayoutNode, row: i64, query: &str, matches: &mut Vec<usize>) {
        for (index, line) in node.snapshot.lines.iter_mut().enumerate() {
            let text: String = line.spans.iter().map(|s| s.text.as_str()).collect();
            let ranges: Vec<_> = text
                .match_indices(query)
                .map(|(start, _)| start..start + query.len())
                .collect();
            if ranges.is_empty() {
                continue;
            }
            if let Ok(row) = usize::try_from(row + index as i64) {
                matches.push(row);
            }
            let mut spans = Vec::new();
            let mut byte = 0;
            for span in &line.spans {
                for (offset, grapheme) in span.text.grapheme_indices(true) {
                    let mut style = span.style;
                    if ranges
                        .iter()
                        .any(|r| r.start < byte + offset + grapheme.len() && r.end > byte + offset)
                    {
                        style.reversed = true;
                    }
                    spans.push(Span {
                        text: grapheme.into(),
                        style,
                        hyperlink: span.hyperlink.clone(),
                    });
                }
                byte += span.text.len();
            }
            line.spans = spans;
        }
        for child in &mut node.snapshot.children {
            Self::visit(&mut child.node, row + child.offset.row, query, matches);
        }
    }
}
impl Component for SearchView {
    fn focusable(&self) -> bool {
        true
    }
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        if children.len() != 1 {
            return Err(ComponentError::InvalidLayout {
                reason: "SearchView requires exactly one child".into(),
            });
        }
        let mut node = children[0].layout(&LayoutContext {
            available_height: None,
            ..*context
        })?;
        self.content = node.snapshot.height;
        let height = usize::from(
            context
                .available_height
                .unwrap_or(self.height)
                .min(self.height),
        );
        let search_rows = usize::from(self.searching && height > 0);
        self.viewport = height.saturating_sub(search_rows);
        self.matches.clear();
        if !self.query.is_empty() {
            Self::visit(&mut node, 0, &self.query, &mut self.matches);
            self.matches.sort_unstable();
            self.matches.dedup();
            self.selected = self.selected.min(self.matches.len().saturating_sub(1));
            if let Some(row) = self.matches.get(self.selected) {
                self.scroll = *row;
            }
        }
        self.scroll = self.scroll.min(self.content.saturating_sub(self.viewport));
        let scroll = i64::try_from(self.scroll).map_err(|_| ComponentError::InvalidLayout {
            reason: "search scroll exceeds i64".into(),
        })?;
        let mut lines = Vec::new();
        if search_rows > 0 {
            lines.push(Line::plain(truncate_text(
                &format!(
                    "搜索: {} [{}/{}]",
                    self.query,
                    if self.matches.is_empty() {
                        0
                    } else {
                        self.selected + 1
                    },
                    self.matches.len()
                ),
                context.width,
                "…",
            )?));
        }
        Ok(LayoutSnapshot {
            width: context.width,
            height,
            lines,
            children: vec![ChildPlacement {
                offset: Offset {
                    column: 0,
                    row: search_rows as i64 - scroll,
                },
                clip: Some(ClipRect {
                    column: 0,
                    row: search_rows as i64,
                    width: context.width,
                    height: self.viewport,
                }),
                node,
            }],
            ..LayoutSnapshot::default()
        })
    }
    fn handle_event(
        &mut self,
        event: &ComponentEvent,
        _host: &mut dyn ComponentHost,
    ) -> ComponentResult<EventResponse> {
        match event {
            ComponentEvent::Key(e)
                if e.kind != KeyKind::Release
                    && e.modifiers.control
                    && e.key == Key::Character('f') =>
            {
                self.searching = true;
            }
            ComponentEvent::Text(text) | ComponentEvent::Paste(text) if self.searching => {
                self.set_query(&(self.query.clone() + text))?;
            }
            ComponentEvent::Key(e) if e.kind != KeyKind::Release => match e.key {
                Key::Escape if self.searching => {
                    self.searching = false;
                    self.query.clear();
                }
                Key::Backspace if self.searching => {
                    if let Some((i, _)) = self.query.grapheme_indices(true).next_back() {
                        let query = self.query[..i].to_owned();
                        self.set_query(&query)?;
                    }
                }
                Key::Enter if self.searching && !self.matches.is_empty() => {
                    self.selected = if e.modifiers.shift {
                        (self.selected + self.matches.len() - 1) % self.matches.len()
                    } else {
                        (self.selected + 1) % self.matches.len()
                    };
                }
                Key::Up => self.scroll = self.scroll.saturating_sub(1),
                Key::Down => self.scroll = self.scroll.saturating_add(1),
                Key::PageUp => self.scroll = self.scroll.saturating_sub(self.viewport.max(1)),
                Key::PageDown => self.scroll = self.scroll.saturating_add(self.viewport.max(1)),
                _ => return Ok(EventResponse::default()),
            },
            _ => return Ok(EventResponse::default()),
        }
        Ok(EventResponse {
            handled: true,
            redraw: true,
            ..EventResponse::default()
        })
    }
}
