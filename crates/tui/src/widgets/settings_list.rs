use crate::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{
        ComponentError, ComponentEvent, ComponentHost, ComponentResult, EventResponse, Key,
        KeyKind, LayoutContext, Line, MouseButton, PointerKind, Span, Style,
    },
    utils::text::{display_width, truncate_text, wrap_text},
};
use std::collections::HashSet;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone)]
pub struct SettingItem {
    pub id: String,
    pub label: String,
    pub current_value: String,
    pub values: Vec<String>,
    pub description: Option<String>,
}
impl SettingItem {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        current_value: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            current_value: current_value.into(),
            values: Vec::new(),
            description: None,
        }
    }
    pub fn with_values(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.values = values.into_iter().map(Into::into).collect();
        self
    }
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}
type ChangeCallback = Box<dyn FnMut(&str, &str)>;

/// 值循环切换与可选标签子串搜索。空 values 表示只读项。
pub struct SettingsList {
    items: Vec<SettingItem>,
    filtered: Vec<usize>,
    selected: usize,
    max_visible: u16,
    start: usize,
    visible: usize,
    list_row: usize,
    search: bool,
    query: String,
    on_change: Option<ChangeCallback>,
    on_cancel: Option<Box<dyn FnMut()>>,
}
impl SettingsList {
    pub fn new(items: Vec<SettingItem>, max_visible: u16) -> ComponentResult<Self> {
        let mut ids = HashSet::new();
        for item in &items {
            if item.id.is_empty() || !ids.insert(item.id.clone()) {
                return Err(ComponentError::InvalidContent {
                    reason: "setting IDs must be nonempty and unique".into(),
                });
            }
            display_width(&item.id)?;
            display_width(&item.label)?;
            display_width(&item.current_value)?;
            for value in &item.values {
                display_width(value)?;
            }
            if let Some(description) = &item.description {
                wrap_text(description, 80)?;
            }
        }
        let filtered = (0..items.len()).collect();
        Ok(Self {
            items,
            filtered,
            selected: 0,
            max_visible,
            start: 0,
            visible: 0,
            list_row: 0,
            search: false,
            query: String::new(),
            on_change: None,
            on_cancel: None,
        })
    }
    pub fn with_search(mut self, enabled: bool) -> Self {
        self.search = enabled;
        self
    }
    pub fn on_change(mut self, callback: impl FnMut(&str, &str) + 'static) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }
    pub fn on_cancel(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_cancel = Some(Box::new(callback));
        self
    }
    pub fn items(&self) -> &[SettingItem] {
        &self.items
    }
    pub fn selected_item(&self) -> Option<&SettingItem> {
        self.filtered
            .get(self.selected)
            .map(|index| &self.items[*index])
    }
    pub fn select_item(&mut self, id: &str) {
        if let Some(index) = self
            .filtered
            .iter()
            .position(|index| self.items[*index].id == id)
        {
            self.selected = index;
        }
    }
    pub fn update_value(&mut self, id: &str, value: impl Into<String>) -> ComponentResult<bool> {
        let value = value.into();
        display_width(&value)?;
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.current_value = value;
            return Ok(true);
        }
        Ok(false)
    }
    pub fn set_filter(&mut self, query: &str) -> ComponentResult<()> {
        display_width(query)?;
        self.query = query.into();
        let query = query.to_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| item.label.to_lowercase().contains(&query).then_some(index))
            .collect();
        self.selected = 0;
        self.visible = 0;
        Ok(())
    }
    fn activate(&mut self) {
        let Some(index) = self.filtered.get(self.selected) else {
            return;
        };
        let item = &mut self.items[*index];
        if item.values.is_empty() {
            return;
        }
        let next = item
            .values
            .iter()
            .position(|value| value == &item.current_value)
            .map_or(0, |index| (index + 1) % item.values.len());
        let value = item.values[next].clone();
        if value != item.current_value {
            item.current_value = value;
            if let Some(callback) = &mut self.on_change {
                callback(&item.id, &item.current_value);
            }
        }
    }
}
impl Component for SettingsList {
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
                reason: "SettingsList does not accept children".into(),
            });
        }
        self.visible = 0;
        self.list_row = 0;
        if context.width == 0 || context.available_height == Some(0) {
            return Ok(LayoutSnapshot::empty(context.width));
        }
        let available = context.available_height.map_or(usize::MAX, usize::from);
        let mut lines = Vec::new();
        if self.search {
            lines.push(Line::plain(truncate_text(
                &format!("搜索: {}", self.query),
                context.width,
                "…",
            )?));
            self.list_row = 1;
        }
        let room = available.saturating_sub(lines.len());
        if self.filtered.is_empty() && room > 0 {
            lines.push(Line::plain(truncate_text(
                "没有匹配设置",
                context.width,
                "…",
            )?));
        } else {
            self.visible = self
                .filtered
                .len()
                .min(usize::from(self.max_visible))
                .min(room);
            self.start = self
                .selected
                .saturating_sub(self.visible / 2)
                .min(self.filtered.len().saturating_sub(self.visible));
            for index in self.start..self.start + self.visible {
                let item = &self.items[self.filtered[index]];
                let text = format!(
                    "{}{}  {}",
                    if index == self.selected { "> " } else { "  " },
                    item.label,
                    item.current_value
                );
                lines.push(Line {
                    spans: vec![Span::styled(
                        truncate_text(&text, context.width, "…")?,
                        Style {
                            reversed: index == self.selected,
                            ..Style::default()
                        },
                    )],
                });
            }
            if let Some(description) = self
                .selected_item()
                .and_then(|item| item.description.as_deref())
            {
                let room = available.saturating_sub(lines.len());
                if room > 0 {
                    lines.extend(
                        wrap_text(description, context.width)?
                            .into_iter()
                            .take(room.min(3))
                            .map(|text| Line {
                                spans: vec![Span::styled(
                                    text,
                                    Style {
                                        dim: true,
                                        ..Style::default()
                                    },
                                )],
                            }),
                    );
                }
            }
        }
        Ok(LayoutSnapshot::from_lines(context.width, lines))
    }
    fn handle_event(
        &mut self,
        event: &ComponentEvent,
        _host: &mut dyn ComponentHost,
    ) -> ComponentResult<EventResponse> {
        match event {
            ComponentEvent::Text(text) | ComponentEvent::Paste(text) if self.search => {
                self.set_filter(&(self.query.clone() + text))?;
            }
            ComponentEvent::Key(event)
                if event.kind != KeyKind::Release && event.modifiers == Default::default() =>
            {
                match event.key {
                    Key::Escape => {
                        if self.search && !self.query.is_empty() {
                            self.set_filter("")?;
                        } else if let Some(callback) = &mut self.on_cancel {
                            callback();
                        } else {
                            return Ok(EventResponse::default());
                        }
                    }
                    Key::Backspace if self.search && !self.query.is_empty() => {
                        let start = self
                            .query
                            .grapheme_indices(true)
                            .next_back()
                            .map_or(0, |(index, _)| index);
                        let query = self.query[..start].to_owned();
                        self.set_filter(&query)?;
                    }
                    Key::Up if !self.filtered.is_empty() => {
                        self.selected = self.selected.saturating_sub(1)
                    }
                    Key::Down if !self.filtered.is_empty() => {
                        self.selected = (self.selected + 1).min(self.filtered.len() - 1)
                    }
                    Key::Home if !self.filtered.is_empty() => self.selected = 0,
                    Key::End if !self.filtered.is_empty() => {
                        self.selected = self.filtered.len() - 1
                    }
                    Key::Enter => self.activate(),
                    Key::Character(' ') if !self.search || self.query.is_empty() => self.activate(),
                    _ => return Ok(EventResponse::default()),
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
                    }
                }
                PointerKind::Press | PointerKind::Click
                    if event.button == Some(MouseButton::Left)
                        && event.row >= self.list_row as i64
                        && (event.row as usize) < self.list_row + self.visible =>
                {
                    self.selected = self.start + event.row as usize - self.list_row;
                    if event.kind == PointerKind::Click {
                        self.activate();
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
