use std::collections::VecDeque;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{
        ComponentError, ComponentEvent, ComponentHost, ComponentResult, Cursor, EventResponse, Key,
        KeyKind, LayoutContext, Line, MouseButton, PointerKind, Position, Span, StateSnapshot,
        Style,
    },
    utils::text::{display_width, prefix_by_width},
};

type SubmitCallback = Box<dyn FnMut(&str)>;

/// 单行输入；光标以 UTF-8 字节索引存储，编辑操作始终使用字素簇边界。
pub struct Input {
    value: String,
    cursor: usize,
    prompt: String,
    placeholder: String,
    style: Style,
    rendered_start: usize,
    prompt_width: usize,
    undo: VecDeque<(String, usize)>,
    on_submit: Option<SubmitCallback>,
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

impl Input {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            prompt: "> ".into(),
            placeholder: String::new(),
            style: Style::default(),
            rendered_start: 0,
            prompt_width: 0,
            undo: VecDeque::new(),
            on_submit: None,
        }
    }
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }
    pub fn with_placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
    pub fn on_submit(mut self, callback: impl FnMut(&str) + 'static) -> Self {
        self.on_submit = Some(Box::new(callback));
        self
    }
    pub fn value(&self) -> &str {
        &self.value
    }
    /// 光标的 UTF-8 字节偏移，不是终端列号。
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn set_value(&mut self, value: impl Into<String>) -> ComponentResult<()> {
        let value = value.into();
        display_width(&value)?;
        self.cursor = value.len();
        self.value = value;
        self.rendered_start = 0;
        self.undo.clear();
        Ok(())
    }
    fn checkpoint(&mut self) {
        if self.undo.len() == 100 {
            self.undo.pop_front();
        }
        self.undo.push_back((self.value.clone(), self.cursor));
    }
    fn boundaries(&self) -> Vec<usize> {
        self.value
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain(std::iter::once(self.value.len()))
            .collect()
    }
    fn previous(&self) -> usize {
        self.boundaries()
            .into_iter()
            .rfind(|index| *index < self.cursor)
            .unwrap_or(0)
    }
    fn next(&self) -> usize {
        self.boundaries()
            .into_iter()
            .find(|index| *index > self.cursor)
            .unwrap_or(self.value.len())
    }
    fn word_left(&self) -> usize {
        let mut target = self.cursor;
        let mut word = false;
        for (index, grapheme) in self.value[..self.cursor].grapheme_indices(true).rev() {
            let whitespace = grapheme.chars().all(char::is_whitespace);
            if word && whitespace {
                break;
            }
            word |= !whitespace;
            target = index;
        }
        target
    }
    fn word_right(&self) -> usize {
        let mut target = self.cursor;
        let mut word = false;
        for (index, grapheme) in self.value[self.cursor..].grapheme_indices(true) {
            let whitespace = grapheme.chars().all(char::is_whitespace);
            if word && whitespace {
                break;
            }
            word |= !whitespace;
            target = self.cursor + index + grapheme.len();
        }
        target
    }
    fn delete(&mut self, start: usize, end: usize) {
        if start == end {
            return;
        }
        self.checkpoint();
        self.value.replace_range(start..end, "");
        self.cursor = start;
        self.align_cursor();
    }
    fn align_cursor(&mut self) {
        self.cursor = self
            .boundaries()
            .into_iter()
            .find(|index| *index >= self.cursor)
            .unwrap_or(self.value.len());
    }
    fn insert(&mut self, text: &str) -> ComponentResult<()> {
        display_width(text)?;
        if !text.is_empty() {
            self.checkpoint();
            self.value.insert_str(self.cursor, text);
            self.cursor += text.len();
            self.align_cursor();
        }
        Ok(())
    }
}

impl Component for Input {
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
                reason: "Input does not accept children".into(),
            });
        }
        if context.width == 0 || context.available_height == Some(0) {
            return Ok(LayoutSnapshot::empty(context.width));
        }
        let prompt = prefix_by_width(&self.prompt, context.width)?;
        self.prompt_width = display_width(&prompt)?;
        let available = usize::from(context.width) - self.prompt_width;
        let mut spans = vec![Span::styled(prompt, self.style)];
        let mut cursor = None;
        self.rendered_start = 0;
        if available > 0 {
            let boundaries = self.boundaries();
            // 留出一个光标单元格；滚动起点始终是完整字素簇的边界。
            while display_width(&self.value[self.rendered_start..self.cursor])? >= available {
                self.rendered_start = *boundaries
                    .iter()
                    .find(|index| **index > self.rendered_start)
                    .expect("cursor has preceding text");
            }
            let cursor_column = display_width(&self.value[self.rendered_start..self.cursor])?;
            let text = if self.value.is_empty() {
                prefix_by_width(&self.placeholder, available as u16)?
            } else {
                prefix_by_width(&self.value[self.rendered_start..], available as u16)?
            };
            let mut style = self.style;
            if self.value.is_empty() {
                style.dim = true;
            }
            spans.push(Span::styled(text, style));
            if context.focused {
                cursor = Some(Cursor {
                    position: Position {
                        column: (self.prompt_width + cursor_column) as u16,
                        row: 0,
                    },
                });
            }
        }
        let mut snapshot = LayoutSnapshot::from_lines(context.width, vec![Line { spans }]);
        snapshot.cursor = cursor;
        Ok(snapshot)
    }

    fn handle_event(
        &mut self,
        event: &ComponentEvent,
        _host: &mut dyn ComponentHost,
    ) -> ComponentResult<EventResponse> {
        match event {
            ComponentEvent::Text(text) => self.insert(text)?,
            ComponentEvent::Paste(text) => {
                // 对齐 TS 单行输入行为：删除换行，制表符替换为四个空格。
                let clean = text.replace(['\r', '\n'], "").replace('\t', "    ");
                self.insert(&clean)?;
            }
            ComponentEvent::Key(event) if event.kind != KeyKind::Release => {
                let modifiers = event.modifiers;
                if modifiers.super_key {
                    return Ok(EventResponse::default());
                }
                let key = event.base_layout_key.unwrap_or(event.key);
                if modifiers.control && !modifiers.alt {
                    match key {
                        Key::Character('a') => self.cursor = 0,
                        Key::Character('e') => self.cursor = self.value.len(),
                        Key::Character('u') => self.delete(0, self.cursor),
                        Key::Character('k') => self.delete(self.cursor, self.value.len()),
                        Key::Character('w') => self.delete(self.word_left(), self.cursor),
                        Key::Character('z') => {
                            if let Some((value, cursor)) = self.undo.pop_back() {
                                self.value = value;
                                self.cursor = cursor;
                            }
                        }
                        Key::Left => self.cursor = self.word_left(),
                        Key::Right => self.cursor = self.word_right(),
                        _ => return Ok(EventResponse::default()),
                    }
                } else if modifiers.alt && !modifiers.control {
                    match key {
                        Key::Character('b') | Key::Left => self.cursor = self.word_left(),
                        Key::Character('f') | Key::Right => self.cursor = self.word_right(),
                        Key::Character('d') | Key::Delete => {
                            self.delete(self.cursor, self.word_right())
                        }
                        Key::Backspace => self.delete(self.word_left(), self.cursor),
                        _ => return Ok(EventResponse::default()),
                    }
                } else if !modifiers.control && !modifiers.alt {
                    match key {
                        Key::Left => self.cursor = self.previous(),
                        Key::Right => self.cursor = self.next(),
                        Key::Home => self.cursor = 0,
                        Key::End => self.cursor = self.value.len(),
                        Key::Backspace => self.delete(self.previous(), self.cursor),
                        Key::Delete => self.delete(self.cursor, self.next()),
                        Key::Enter if !modifiers.shift => {
                            if let Some(callback) = &mut self.on_submit {
                                callback(&self.value);
                            }
                        }
                        _ => return Ok(EventResponse::default()),
                    }
                } else {
                    return Ok(EventResponse::default());
                }
            }
            ComponentEvent::Pointer(event)
                if event.kind == PointerKind::Press
                    && event.button == Some(MouseButton::Left)
                    && event.row == 0 =>
            {
                let target = (i64::from(event.column) - self.prompt_width as i64).max(0) as usize;
                let mut column = 0;
                self.cursor = self.value.len();
                for (index, grapheme) in self.value[self.rendered_start..].grapheme_indices(true) {
                    let next = column + UnicodeWidthStr::width(grapheme);
                    if target < next {
                        self.cursor = self.rendered_start + index;
                        break;
                    }
                    column = next;
                }
                return Ok(EventResponse {
                    handled: true,
                    redraw: true,
                    request_focus: true,
                    ..EventResponse::default()
                });
            }
            _ => return Ok(EventResponse::default()),
        }
        Ok(EventResponse {
            handled: true,
            redraw: true,
            ..EventResponse::default()
        })
    }

    fn save_state(&self) -> ComponentResult<Option<StateSnapshot>> {
        let mut payload = (self.cursor as u64).to_le_bytes().to_vec();
        payload.extend_from_slice(self.value.as_bytes());
        Ok(Some(StateSnapshot {
            schema_version: 1,
            encoding: "application/x-bi-input".into(),
            payload,
        }))
    }
    fn restore_state(&mut self, state: &StateSnapshot) -> ComponentResult<()> {
        if state.schema_version != 1 {
            return Err(ComponentError::UnsupportedState {
                version: state.schema_version,
            });
        }
        let invalid = || ComponentError::InvalidContent {
            reason: "invalid Input state".into(),
        };
        if state.encoding != "application/x-bi-input" || state.payload.len() < 8 {
            return Err(invalid());
        }
        let cursor = usize::try_from(u64::from_le_bytes(
            state.payload[..8].try_into().map_err(|_| invalid())?,
        ))
        .map_err(|_| invalid())?;
        let value = std::str::from_utf8(&state.payload[8..]).map_err(|_| invalid())?;
        display_width(value)?;
        if cursor > value.len()
            || !(cursor == value.len()
                || value
                    .grapheme_indices(true)
                    .any(|(index, _)| index == cursor))
        {
            return Err(invalid());
        }
        self.value = value.to_owned();
        self.cursor = cursor;
        self.rendered_start = 0;
        self.undo.clear();
        Ok(())
    }
}
