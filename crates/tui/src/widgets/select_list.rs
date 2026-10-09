use crate::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{
        ComponentError, ComponentEvent, ComponentHost, ComponentResult, EventResponse, Key,
        KeyKind, LayoutContext, Line, MouseButton, PointerKind, Span, Style,
    },
    utils::text::truncate_text,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

impl SelectItem {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
        }
    }
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

type SelectCallback = Box<dyn FnMut(&SelectItem)>;

/// 值前缀过滤、循环键盘导航与鼠标选择。
pub struct SelectList {
    items: Vec<SelectItem>,
    filtered: Vec<usize>,
    selected: usize,
    max_visible: u16,
    start: usize,
    visible: usize,
    on_select: Option<SelectCallback>,
    on_cancel: Option<Box<dyn FnMut()>>,
    fuzzy: bool,
}

impl SelectList {
    pub fn new(items: Vec<SelectItem>, max_visible: u16) -> Self {
        let filtered = (0..items.len()).collect();
        Self {
            items,
            filtered,
            selected: 0,
            max_visible,
            start: 0,
            visible: 0,
            on_select: None,
            on_cancel: None,
            fuzzy: false,
        }
    }
    pub fn on_select(mut self, callback: impl FnMut(&SelectItem) + 'static) -> Self {
        self.on_select = Some(Box::new(callback));
        self
    }
    pub fn on_cancel(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_cancel = Some(Box::new(callback));
        self
    }
    pub fn selected_item(&self) -> Option<&SelectItem> {
        self.filtered
            .get(self.selected)
            .map(|index| &self.items[*index])
    }
    pub fn with_fuzzy(mut self, enabled: bool) -> Self {
        self.fuzzy = enabled;
        self
    }
    pub fn set_selected_index(&mut self, index: usize) {
        self.selected = index.min(self.filtered.len().saturating_sub(1));
    }
    pub fn set_filter(&mut self, filter: &str) {
        if self.fuzzy {
            let mut matches: Vec<_> = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    crate::utils::fuzzy::fuzzy_score(
                        filter,
                        &format!("{} {}", item.value, item.label),
                    )
                    .map(|score| (score, i))
                })
                .collect();
            matches.sort_by_key(|item| std::cmp::Reverse(item.0));
            self.filtered = matches.into_iter().map(|(_, i)| i).collect();
            self.selected = 0;
            self.start = 0;
            self.visible = 0;
            return;
        }
        let filter = filter.to_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                item.value
                    .to_lowercase()
                    .starts_with(&filter)
                    .then_some(index)
            })
            .collect();
        self.selected = 0;
        self.start = 0;
        self.visible = 0;
    }
    fn submit(&mut self) {
        if let Some(index) = self.filtered.get(self.selected)
            && let Some(callback) = &mut self.on_select
        {
            callback(&self.items[*index]);
        }
    }
}

impl Component for SelectList {
    fn focusable(&self) -> bool {
        true
    }
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        if !children.is_empty() {
            return Err(ComponentError::InvalidLayout {
                reason: "SelectList does not accept children".into(),
            });
        }
        self.visible = usize::from(
            context
                .available_height
                .unwrap_or(self.max_visible)
                .min(self.max_visible),
        );
        if context.width == 0 || self.visible == 0 {
            self.visible = 0;
            return Ok(LayoutSnapshot::empty(context.width));
        }
        if self.filtered.is_empty() {
            self.visible = 0;
            return Ok(LayoutSnapshot::from_lines(
                context.width,
                vec![Line::plain(truncate_text(
                    "没有匹配项",
                    context.width,
                    "…",
                )?)],
            ));
        }
        self.visible = self.visible.min(self.filtered.len());
        self.start = self
            .selected
            .saturating_sub(self.visible / 2)
            .min(self.filtered.len() - self.visible);
        let mut lines = Vec::with_capacity(self.visible);
        for index in self.start..self.start + self.visible {
            let item = &self.items[self.filtered[index]];
            let prefix = if index == self.selected { "> " } else { "  " };
            let label = item.label.replace(['\r', '\n'], " ");
            let description = item
                .description
                .as_deref()
                .unwrap_or_default()
                .replace(['\r', '\n'], " ");
            let label_width = self
                .filtered
                .iter()
                .map(|i| {
                    crate::utils::text::display_width(
                        &self.items[*i].label.replace(['\r', '\n'], " "),
                    )
                    .unwrap_or(0)
                })
                .max()
                .unwrap_or(0)
                .min(usize::from(context.width) / 2);
            let text = if description.is_empty() || context.width < 16 {
                format!("{prefix}{label}")
            } else {
                let label = truncate_text(&label, label_width as u16, "…")?;
                let padding =
                    label_width.saturating_sub(crate::utils::text::display_width(&label)?);
                format!("{prefix}{label}{}  {description}", " ".repeat(padding))
            };
            let style = Style {
                reversed: index == self.selected,
                ..Style::default()
            };
            lines.push(Line {
                spans: vec![Span::styled(
                    truncate_text(&text, context.width, "…")?,
                    style,
                )],
            });
        }
        Ok(LayoutSnapshot::from_lines(context.width, lines))
    }
    fn handle_event(
        &mut self,
        event: &ComponentEvent,
        _host: &mut dyn ComponentHost,
    ) -> ComponentResult<EventResponse> {
        match event {
            ComponentEvent::Key(event)
                if event.kind != KeyKind::Release && event.modifiers == Default::default() =>
            {
                if event.key == Key::Escape {
                    if let Some(callback) = &mut self.on_cancel {
                        callback();
                    } else {
                        return Ok(EventResponse::default());
                    }
                } else {
                    if self.filtered.is_empty() {
                        return Ok(EventResponse::default());
                    }
                    match event.key {
                        Key::Up => {
                            self.selected = if self.selected == 0 {
                                self.filtered.len() - 1
                            } else {
                                self.selected - 1
                            }
                        }
                        Key::Down => self.selected = (self.selected + 1) % self.filtered.len(),
                        Key::Home => self.selected = 0,
                        Key::End => self.selected = self.filtered.len() - 1,
                        Key::Enter => self.submit(),
                        _ => return Ok(EventResponse::default()),
                    }
                }
            }
            ComponentEvent::Pointer(event) if !self.filtered.is_empty() => match event.kind {
                PointerKind::Scroll { rows, .. } if rows != 0 => {
                    self.selected = if rows < 0 {
                        self.selected.saturating_sub(rows.unsigned_abs() as usize)
                    } else {
                        self.selected
                            .saturating_add(rows as usize)
                            .min(self.filtered.len() - 1)
                    };
                }
                PointerKind::Press | PointerKind::Click
                    if event.button == Some(MouseButton::Left)
                        && event.row >= 0
                        && (event.row as usize) < self.visible =>
                {
                    self.selected = self.start + event.row as usize;
                    if event.kind == PointerKind::Click {
                        self.submit();
                    }
                    return Ok(EventResponse {
                        handled: true,
                        redraw: true,
                        request_focus: true,
                        ..EventResponse::default()
                    });
                }
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
