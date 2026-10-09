use crate::{
    component::{ChildPlacement, Component, ComponentNode, LayoutSnapshot},
    protocol::{
        ClipRect, ComponentError, ComponentEvent, ComponentHost, ComponentResult, EventResponse,
        Key, KeyKind, LayoutContext, Line, MouseButton, Offset, PointerCapture, PointerKind,
    },
};

/// 固定期望高度的纵向视口，恰好接收一个子组件。
/// 在滚动边界不消费滚轮事件，让祖先视口继续处理。
#[derive(Debug, Clone)]
pub struct ScrollView {
    height: u16,
    scroll_top: usize,
    content_height: usize,
    viewport_height: usize,
    follow_end: bool,
    following_end: bool,
    scrollbar: bool,
    layout_width: u16,
    dragging_scrollbar: bool,
}

impl ScrollView {
    pub fn new(height: u16) -> Self {
        Self {
            height,
            scroll_top: 0,
            content_height: 0,
            viewport_height: 0,
            follow_end: false,
            following_end: false,
            scrollbar: false,
            layout_width: 0,
            dragging_scrollbar: false,
        }
    }
    pub fn with_follow_end(mut self, enabled: bool) -> Self {
        self.follow_end = enabled;
        self.following_end = enabled;
        self
    }
    pub fn with_scrollbar(mut self, enabled: bool) -> Self {
        self.scrollbar = enabled;
        self
    }
    pub fn scroll_top(&self) -> usize {
        self.scroll_top
    }
    pub fn content_height(&self) -> usize {
        self.content_height
    }
    pub fn viewport_height(&self) -> usize {
        self.viewport_height
    }
    pub fn is_following_end(&self) -> bool {
        self.following_end
    }
    pub fn set_height(&mut self, height: u16) {
        self.height = height;
    }
    pub fn scroll_to(&mut self, row: usize) {
        self.scroll_top = row.min(self.content_height.saturating_sub(self.viewport_height));
        self.following_end = self.follow_end
            && self.scroll_top == self.content_height.saturating_sub(self.viewport_height);
    }
    pub fn scroll_by(&mut self, rows: i64) -> bool {
        let previous = self.scroll_top;
        let next = if rows < 0 {
            previous.saturating_sub(rows.unsigned_abs() as usize)
        } else {
            previous.saturating_add(rows as usize)
        };
        self.scroll_to(next);
        self.scroll_top != previous
    }
}

impl Component for ScrollView {
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
                reason: "ScrollView requires exactly one child".into(),
            });
        }
        self.layout_width = context.width;
        let child_width = context.width.saturating_sub(u16::from(self.scrollbar));
        let child_context = LayoutContext {
            width: child_width,
            available_height: None,
            ..*context
        };
        let node = children[0].layout(&child_context)?;
        self.content_height = node.snapshot.height;
        self.viewport_height = usize::from(
            context
                .available_height
                .map_or(self.height, |limit| self.height.min(limit)),
        );
        let maximum = self.content_height.saturating_sub(self.viewport_height);
        self.scroll_top = if self.following_end {
            maximum
        } else {
            self.scroll_top.min(maximum)
        };
        let row = i64::try_from(self.scroll_top).map_err(|_| ComponentError::InvalidLayout {
            reason: "scroll offset exceeds i64".into(),
        })?;
        let mut lines = Vec::new();
        if self.scrollbar && context.width > 0 {
            let thumb_height = self
                .viewport_height
                .saturating_mul(self.viewport_height)
                .checked_div(self.content_height)
                .unwrap_or(self.viewport_height)
                .max(1)
                .min(self.viewport_height);
            let thumb_top = self
                .scroll_top
                .saturating_mul(self.viewport_height.saturating_sub(thumb_height))
                .checked_div(maximum)
                .unwrap_or(0);
            for row in 0..self.viewport_height {
                lines.push(Line::plain(format!(
                    "{}{}",
                    " ".repeat(usize::from(child_width)),
                    if row >= thumb_top && row < thumb_top + thumb_height {
                        "█"
                    } else {
                        "│"
                    }
                )));
            }
        }
        Ok(LayoutSnapshot {
            lines,
            width: context.width,
            height: self.viewport_height,
            children: vec![ChildPlacement {
                offset: Offset {
                    column: 0,
                    row: -row,
                },
                clip: Some(ClipRect {
                    column: 0,
                    row: 0,
                    width: child_width,
                    height: self.viewport_height,
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
        let changed = match event {
            ComponentEvent::Pointer(event) => match event.kind {
                PointerKind::Drag if self.dragging_scrollbar => {
                    let maximum = self.content_height.saturating_sub(self.viewport_height);
                    let row =
                        (event.row.max(0) as usize).min(self.viewport_height.saturating_sub(1));
                    self.scroll_to(
                        row.saturating_mul(maximum) / self.viewport_height.saturating_sub(1).max(1),
                    );
                    return Ok(EventResponse {
                        handled: true,
                        redraw: true,
                        ..EventResponse::default()
                    });
                }
                PointerKind::Release if self.dragging_scrollbar => {
                    self.dragging_scrollbar = false;
                    return Ok(EventResponse {
                        handled: true,
                        pointer_capture: PointerCapture::Release,
                        ..EventResponse::default()
                    });
                }
                PointerKind::Scroll { rows, columns } => {
                    let old = self.scroll_top;
                    let changed = self.scroll_by(i64::from(rows));
                    let consumed = self.scroll_top as i64 - old as i64;
                    let remainder = (i64::from(rows) - consumed) as i32;
                    return Ok(EventResponse {
                        handled: remainder == 0 && columns == 0 && rows != 0,
                        redraw: changed,
                        scroll_remainder: Some(remainder),
                        ..EventResponse::default()
                    });
                }
                PointerKind::Press => {
                    if self.scrollbar
                        && event.button == Some(MouseButton::Left)
                        && self.layout_width > 0
                        && event.column == i32::from(self.layout_width.saturating_sub(1))
                        && event.row >= 0
                        && (event.row as usize) < self.viewport_height
                    {
                        let maximum = self.content_height.saturating_sub(self.viewport_height);
                        self.scroll_to(
                            (event.row as usize).saturating_mul(maximum)
                                / self.viewport_height.saturating_sub(1).max(1),
                        );
                        self.dragging_scrollbar = true;
                        return Ok(EventResponse {
                            handled: true,
                            redraw: true,
                            request_focus: true,
                            pointer_capture: PointerCapture::Acquire,
                            ..EventResponse::default()
                        });
                    }
                    return Ok(EventResponse {
                        request_focus: true,
                        ..EventResponse::default()
                    });
                }
                _ => return Ok(EventResponse::default()),
            },
            ComponentEvent::Key(event)
                if event.kind != KeyKind::Release && event.modifiers == Default::default() =>
            {
                match event.key {
                    Key::Up => self.scroll_by(-1),
                    Key::Down => self.scroll_by(1),
                    Key::PageUp => self.scroll_by(-(self.viewport_height.max(1) as i64)),
                    Key::PageDown => self.scroll_by(self.viewport_height.max(1) as i64),
                    Key::Home => {
                        let previous = self.scroll_top;
                        self.scroll_to(0);
                        self.scroll_top != previous
                    }
                    Key::End => {
                        let previous = self.scroll_top;
                        self.scroll_to(usize::MAX);
                        self.scroll_top != previous
                    }
                    _ => return Ok(EventResponse::default()),
                }
            }
            _ => return Ok(EventResponse::default()),
        };
        Ok(EventResponse {
            handled: changed,
            redraw: changed,
            ..EventResponse::default()
        })
    }
}
