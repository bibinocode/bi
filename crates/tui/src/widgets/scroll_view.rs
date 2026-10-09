use crate::{
    component::{ChildPlacement, Component, ComponentNode, LayoutSnapshot},
    protocol::{
        ClipRect, ComponentError, ComponentEvent, ComponentHost, ComponentResult, EventResponse,
        Key, KeyKind, LayoutContext, Offset, PointerKind,
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
        }
    }
    pub fn with_follow_end(mut self, enabled: bool) -> Self {
        self.follow_end = enabled;
        self.following_end = enabled;
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
        let child_context = LayoutContext {
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
        Ok(LayoutSnapshot {
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
                    width: context.width,
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
                PointerKind::Scroll { rows, .. } => self.scroll_by(i64::from(rows)),
                PointerKind::Press => {
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
