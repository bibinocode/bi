use crate::{
    autocomplete::{AutocompleteProvider, Completion},
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{
        ComponentError, ComponentEvent, ComponentHost, ComponentResult, Cursor, EventResponse, Key,
        KeyKind, LayoutContext, Line, MouseButton, PointerKind, Position, Span, StateSnapshot,
        Style,
    },
    utils::text::{truncate_text, validate_text},
};
use std::{collections::VecDeque, ops::Range};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone)]
struct Paste {
    range: Range<usize>,
    id: u64,
    text: String,
}
#[derive(Clone, Default)]
struct Document {
    text: String,
    cursor: usize,
    pastes: Vec<Paste>,
    paste_id: u64,
}
#[derive(Clone)]
struct Unit {
    range: Range<usize>,
    text: String,
    width: usize,
}
#[derive(Clone)]
struct Row {
    start: usize,
    end: usize,
    units: Vec<Unit>,
}
type TextCallback = Box<dyn FnMut(&str)>;

/// 多行纯文本编辑器。text() 返回可编辑文本，expanded_text() 展开受跟踪的粘贴标记。
pub struct Editor {
    document: Document,
    undo: VecDeque<Document>,
    max_height: u16,
    scroll_top: usize,
    layout_width: u16,
    rows: Vec<Row>,
    preferred_column: Option<usize>,
    provider: Option<Box<dyn AutocompleteProvider>>,
    auto_complete: bool,
    completions: Vec<Completion>,
    selected_completion: usize,
    menu_start: usize,
    menu_row: usize,
    menu_visible: usize,
    history: VecDeque<String>,
    history_index: Option<usize>,
    draft: Option<Document>,
    on_submit: Option<TextCallback>,
    on_change: Option<TextCallback>,
    kill_ring: crate::utils::kill_ring::KillRing,
    last_kill: bool,
    yank: Option<Range<usize>>,
    last_insert: Option<(std::time::Instant, usize)>,
    typing: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
impl Editor {
    pub fn new() -> Self {
        Self {
            document: Document::default(),
            undo: VecDeque::new(),
            max_height: 8,
            scroll_top: 0,
            layout_width: 0,
            rows: Vec::new(),
            preferred_column: None,
            provider: None,
            auto_complete: false,
            completions: Vec::new(),
            selected_completion: 0,
            menu_start: 0,
            menu_row: 0,
            menu_visible: 0,
            history: VecDeque::new(),
            history_index: None,
            draft: None,
            on_submit: None,
            on_change: None,
            kill_ring: Default::default(),
            last_kill: false,
            yank: None,
            last_insert: None,
            typing: false,
        }
    }
    /// 包含补全菜单的最大可见行数；零表示不显示。
    pub fn with_height(mut self, height: u16) -> Self {
        self.max_height = height;
        self
    }
    pub fn with_autocomplete(mut self, provider: impl AutocompleteProvider + 'static) -> Self {
        self.provider = Some(Box::new(provider));
        self
    }
    /// 输入文本后更新候选；自动请求只显示菜单，不自动替换文本。
    pub fn with_auto_complete(mut self, enabled: bool) -> Self {
        self.auto_complete = enabled;
        self
    }
    pub fn on_submit(mut self, callback: impl FnMut(&str) + 'static) -> Self {
        self.on_submit = Some(Box::new(callback));
        self
    }
    /// 回调参数为 expanded_text()，不含内部粘贴标记。
    pub fn on_change(mut self, callback: impl FnMut(&str) + 'static) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }
    pub fn text(&self) -> &str {
        &self.document.text
    }
    pub fn cursor(&self) -> usize {
        self.document.cursor
    }
    pub fn expanded_text(&self) -> String {
        let mut result = String::new();
        let mut end = 0;
        for paste in &self.document.pastes {
            result.push_str(&self.document.text[end..paste.range.start]);
            result.push_str(&paste.text);
            end = paste.range.end;
        }
        result.push_str(&self.document.text[end..]);
        result
    }
    pub fn set_text(&mut self, text: impl Into<String>) -> ComponentResult<()> {
        let text = normalize(&text.into())?;
        self.document = Document {
            cursor: text.len(),
            text,
            ..Document::default()
        };
        self.undo.clear();
        self.last_insert = None;
        self.last_kill = false;
        self.yank = None;
        self.history_index = None;
        self.draft = None;
        self.scroll_top = 0;
        self.changed();
        Ok(())
    }
    pub fn add_to_history(&mut self, text: impl Into<String>) -> ComponentResult<()> {
        let text = normalize(&text.into())?.trim().to_owned();
        if !text.is_empty() && self.history.front() != Some(&text) {
            self.history.push_front(text);
            self.history.truncate(100);
        }
        Ok(())
    }
    pub fn insert_text(&mut self, text: &str) -> ComponentResult<()> {
        self.typing = false;
        let text = normalize(text)?;
        self.edit(self.document.cursor..self.document.cursor, &text)?;
        Ok(())
    }
    fn boundaries(&self) -> Vec<usize> {
        segment_boundaries(&self.document)
    }
    fn previous(&self) -> usize {
        self.boundaries()
            .into_iter()
            .rfind(|index| *index < self.document.cursor)
            .unwrap_or(0)
    }
    fn next(&self) -> usize {
        self.boundaries()
            .into_iter()
            .find(|index| *index > self.document.cursor)
            .unwrap_or(self.document.text.len())
    }
    fn checkpoint(&mut self) {
        if self.undo.len() == 100 {
            self.undo.pop_front();
        }
        self.undo.push_back(self.document.clone());
    }
    fn changed(&mut self) {
        self.rows.clear();
        self.completions.clear();
        self.menu_visible = 0;
        self.preferred_column = None;
        if self.on_change.is_some() {
            let text = self.expanded_text();
            if let Some(callback) = &mut self.on_change {
                callback(&text);
            }
        }
    }
    fn edit(&mut self, range: Range<usize>, text: &str) -> ComponentResult<()> {
        self.edit_internal(range, text, true)
    }
    fn edit_internal(
        &mut self,
        range: Range<usize>,
        text: &str,
        notify: bool,
    ) -> ComponentResult<()> {
        validate_text(text, true)?;
        let boundaries = self.boundaries();
        if range.start > range.end
            || !boundaries.contains(&range.start)
            || !boundaries.contains(&range.end)
        {
            return Err(ComponentError::InvalidContent {
                reason: "edit range is not on an editor segment boundary".into(),
            });
        }
        if range.is_empty() && text.is_empty() {
            return Ok(());
        }
        let merge = self.typing
            && range.is_empty()
            && self.last_insert.is_some_and(|(at, cursor)| {
                cursor == range.start && at.elapsed() < std::time::Duration::from_millis(500)
            });
        if !merge {
            self.checkpoint();
        }
        self.last_insert = None;
        self.document
            .pastes
            .retain(|paste| range.end <= paste.range.start || range.start >= paste.range.end);
        for paste in &mut self.document.pastes {
            if paste.range.start >= range.end {
                paste.range.start = paste.range.start - range.len() + text.len();
                paste.range.end = paste.range.end - range.len() + text.len();
            }
        }
        self.document.text.replace_range(range.clone(), text);
        self.document.cursor = range.start + text.len();
        self.document.cursor = self
            .boundaries()
            .into_iter()
            .find(|index| *index >= self.document.cursor)
            .unwrap_or(self.document.text.len());
        if self.typing {
            self.last_insert = Some((std::time::Instant::now(), self.document.cursor));
        }
        self.history_index = None;
        self.draft = None;
        if notify {
            self.changed();
        } else {
            self.rows.clear();
            self.completions.clear();
            self.menu_visible = 0;
            self.preferred_column = None;
        }
        Ok(())
    }
    fn kill(&mut self, range: Range<usize>, backward: bool) -> ComponentResult<()> {
        let mut text = String::new();
        let mut pos = range.start;
        for paste in &self.document.pastes {
            if paste.range.start >= range.start && paste.range.end <= range.end {
                text.push_str(&self.document.text[pos..paste.range.start]);
                text.push_str(&paste.text);
                pos = paste.range.end;
            }
        }
        text.push_str(&self.document.text[pos..range.end]);
        self.edit(range, "")?;
        self.kill_ring.push(text, backward, self.last_kill);
        self.last_kill = true;
        Ok(())
    }
    fn yank(&mut self, rotate: bool) -> ComponentResult<()> {
        let range = if rotate {
            let Some(range) = self.yank.clone() else {
                return Ok(());
            };
            self.kill_ring.rotate();
            range
        } else {
            self.document.cursor..self.document.cursor
        };
        let Some(text) = self.kill_ring.peek().map(str::to_owned) else {
            return Ok(());
        };
        if !self.boundaries().contains(&range.start) || !self.boundaries().contains(&range.end) {
            self.yank = None;
            return Ok(());
        }
        self.edit(range.clone(), &text)?;
        self.yank = Some(range.start..range.start + text.len());
        Ok(())
    }
    fn word_left(&self) -> usize {
        let mut target = self.document.cursor;
        let mut word = false;
        for pair in self.boundaries().windows(2).rev() {
            if pair[1] > self.document.cursor {
                continue;
            }
            let whitespace = self.document.text[pair[0]..pair[1]]
                .chars()
                .all(char::is_whitespace);
            if word && whitespace {
                break;
            }
            word |= !whitespace;
            target = pair[0];
        }
        target
    }
    fn word_right(&self) -> usize {
        let mut target = self.document.cursor;
        let mut word = false;
        for pair in self.boundaries().windows(2) {
            if pair[0] < self.document.cursor {
                continue;
            }
            let whitespace = self.document.text[pair[0]..pair[1]]
                .chars()
                .all(char::is_whitespace);
            if word && whitespace {
                break;
            }
            word |= !whitespace;
            target = pair[1];
        }
        target
    }
    fn paste(&mut self, text: &str) -> ComponentResult<()> {
        let text = normalize(text)?;
        let count = text.split('\n').count();
        if count <= 10 {
            return self.insert_text(&text);
        }
        let id = self.document.paste_id.checked_add(1).ok_or_else(|| {
            ComponentError::OperationFailed {
                message: "paste ID exhausted".into(),
            }
        })?;
        let marker = format!("[paste #{id} +{count} lines]");
        let start = self.document.cursor;
        self.edit_internal(start..start, &marker, false)?;
        self.document.paste_id = id;
        self.document.pastes.push(Paste {
            range: start..start + marker.len(),
            id,
            text,
        });
        self.document.pastes.sort_by_key(|paste| paste.range.start);
        // edit 的中间标记不应该作为业务文本发出。
        self.changed();
        Ok(())
    }
    fn build_rows(&self, width: u16) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut row = Row {
            start: 0,
            end: 0,
            units: Vec::new(),
        };
        let mut column = 0;
        let boundaries = self.boundaries();
        for pair in boundaries.windows(2) {
            let range = pair[0]..pair[1];
            let source = &self.document.text[range.clone()];
            if source == "\n" {
                row.end = range.start;
                rows.push(row);
                row = Row {
                    start: range.end,
                    end: range.end,
                    units: Vec::new(),
                };
                column = 0;
                continue;
            }
            let text = if UnicodeWidthStr::width(source) > usize::from(width) {
                if self
                    .document
                    .pastes
                    .iter()
                    .any(|paste| paste.range == range)
                {
                    truncate_text(source, width, "…").expect("validated paste marker")
                } else {
                    "�".into()
                }
            } else {
                source.to_owned()
            };
            let measured = UnicodeWidthStr::width(text.as_str());
            if column + measured > usize::from(width) && !row.units.is_empty() {
                row.end = range.start;
                rows.push(row);
                row = Row {
                    start: range.start,
                    end: range.start,
                    units: Vec::new(),
                };
                column = 0;
            }
            column += measured;
            row.end = range.end;
            row.units.push(Unit {
                range,
                text,
                width: measured,
            });
        }
        rows.push(row);
        if column >= usize::from(width) {
            rows.push(Row {
                start: self.document.text.len(),
                end: self.document.text.len(),
                units: Vec::new(),
            });
        }
        rows
    }
    fn cursor_position(&self, rows: &[Row]) -> (usize, usize) {
        let row = rows
            .iter()
            .rposition(|row| row.start <= self.document.cursor && self.document.cursor <= row.end)
            .unwrap_or(0);
        let column = rows[row]
            .units
            .iter()
            .take_while(|unit| unit.range.end <= self.document.cursor)
            .map(|unit| unit.width)
            .sum();
        (row, column)
    }
    fn at_column(row: &Row, column: usize) -> usize {
        let mut used = 0;
        for unit in &row.units {
            if used + unit.width > column {
                return unit.range.start;
            }
            used += unit.width;
        }
        row.end
    }
    fn move_vertical(&mut self, direction: i32) {
        let rows = self.build_rows(self.layout_width.max(1));
        let (current, column) = self.cursor_position(&rows);
        let column = self.preferred_column.unwrap_or(column);
        let target = if direction < 0 {
            current.saturating_sub(direction.unsigned_abs() as usize)
        } else {
            current
                .saturating_add(direction as usize)
                .min(rows.len() - 1)
        };
        self.document.cursor = Self::at_column(&rows[target], column);
        self.preferred_column = Some(column);
    }
    fn navigate_history(&mut self, older: bool) -> bool {
        if self.history.is_empty() {
            return false;
        }
        if older {
            let index = self.history_index.map_or(0, |index| index + 1);
            if index >= self.history.len() {
                return false;
            }
            if self.history_index.is_none() {
                self.draft = Some(self.document.clone());
            }
            self.history_index = Some(index);
            self.document = Document {
                text: self.history[index].clone(),
                ..Document::default()
            };
        } else if let Some(index) = self.history_index {
            if index == 0 {
                self.document = self.draft.take().unwrap_or_default();
                self.history_index = None;
            } else {
                self.history_index = Some(index - 1);
                let text = self.history[index - 1].clone();
                self.document = Document {
                    cursor: text.len(),
                    text,
                    ..Document::default()
                };
            }
        } else {
            return false;
        }
        self.changed();
        true
    }
    fn complete(&mut self, accept_single: bool) -> ComponentResult<bool> {
        let Some(provider) = &self.provider else {
            return Ok(false);
        };
        let suggestions = provider.suggestions(&self.document.text, self.document.cursor)?;
        let boundaries = self.boundaries();
        for completion in &suggestions {
            if completion.range.start > completion.range.end
                || !boundaries.contains(&completion.range.start)
                || !boundaries.contains(&completion.range.end)
            {
                return Err(ComponentError::InvalidContent {
                    reason: "completion range is not on an editor segment boundary".into(),
                });
            }
            validate_text(&completion.replacement, true)?;
            validate_text(&completion.label, false)?;
            if let Some(description) = &completion.description {
                validate_text(description, false)?;
            }
        }
        self.completions = suggestions.into_iter().take(64).collect();
        self.selected_completion = 0;
        if accept_single && self.completions.len() == 1 {
            self.accept_completion()?;
            return Ok(true);
        }
        Ok(!self.completions.is_empty())
    }
    fn accept_completion(&mut self) -> ComponentResult<()> {
        if let Some(completion) = self.completions.get(self.selected_completion).cloned() {
            self.edit(completion.range, &completion.replacement)?;
        }
        Ok(())
    }
    fn submit(&mut self) -> ComponentResult<()> {
        let text = self.expanded_text().trim().to_owned();
        self.add_to_history(text.clone())?;
        self.document = Document::default();
        self.undo.clear();
        self.history_index = None;
        self.draft = None;
        self.scroll_top = 0;
        self.changed();
        if let Some(callback) = &mut self.on_submit {
            callback(&text);
        }
        Ok(())
    }
}

impl Component for Editor {
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
                reason: "Editor does not accept children".into(),
            });
        }
        self.layout_width = context.width;
        self.menu_visible = 0;
        let available = usize::from(
            context
                .available_height
                .unwrap_or(self.max_height)
                .min(self.max_height),
        );
        if context.width == 0 || available == 0 {
            self.rows.clear();
            return Ok(LayoutSnapshot::empty(context.width));
        }
        self.rows = self.build_rows(context.width);
        let (cursor_row, column) = self.cursor_position(&self.rows);
        let menu_height = self
            .completions
            .len()
            .min(5)
            .min(available.saturating_sub(1));
        let text_height = self.rows.len().min(available - menu_height);
        self.scroll_top = self
            .scroll_top
            .min(self.rows.len().saturating_sub(text_height));
        if cursor_row < self.scroll_top {
            self.scroll_top = cursor_row;
        }
        if cursor_row >= self.scroll_top + text_height {
            self.scroll_top = cursor_row + 1 - text_height;
        }
        let mut lines: Vec<Line> = self
            .rows
            .iter()
            .skip(self.scroll_top)
            .take(text_height)
            .map(|row| {
                Line::plain(
                    row.units
                        .iter()
                        .map(|unit| unit.text.as_str())
                        .collect::<String>(),
                )
            })
            .collect();
        self.menu_row = lines.len();
        self.menu_visible = menu_height;
        self.menu_start = self
            .selected_completion
            .saturating_sub(menu_height / 2)
            .min(self.completions.len().saturating_sub(menu_height));
        for index in self.menu_start..self.menu_start + menu_height {
            let completion = &self.completions[index];
            let text = format!(
                "{}{}{}",
                if index == self.selected_completion {
                    "> "
                } else {
                    "  "
                },
                completion.label,
                completion
                    .description
                    .as_ref()
                    .map_or(String::new(), |text| format!("  {text}"))
            );
            lines.push(Line {
                spans: vec![Span::styled(
                    truncate_text(&text, context.width, "…")?,
                    Style {
                        reversed: index == self.selected_completion,
                        ..Style::default()
                    },
                )],
            });
        }
        let mut snapshot = LayoutSnapshot::from_lines(context.width, lines);
        if context.focused {
            snapshot.cursor = Some(Cursor {
                position: Position {
                    column: column.min(usize::from(context.width) - 1) as u16,
                    row: cursor_row - self.scroll_top,
                },
            });
        }
        Ok(snapshot)
    }
    fn handle_event(
        &mut self,
        event: &ComponentEvent,
        _host: &mut dyn ComponentHost,
    ) -> ComponentResult<EventResponse> {
        self.typing = matches!(event, ComponentEvent::Text(_));
        if !self.typing
            && !matches!(event, ComponentEvent::Key(e) if !e.modifiers.control && !e.modifiers.alt && matches!(e.key, Key::Character(_)))
        {
            self.last_insert = None;
        }
        let killing = matches!(event, ComponentEvent::Key(e) if e.kind != KeyKind::Release && ((e.modifiers.control && matches!(e.key, Key::Character('u'|'k'|'w'))) || (e.modifiers.alt && matches!(e.key, Key::Character('d') | Key::Backspace | Key::Delete))));
        if !killing {
            self.last_kill = false;
        }
        if !matches!(event, ComponentEvent::Key(e) if (e.modifiers.control || e.modifiers.alt) && e.key == Key::Character('y'))
        {
            self.yank = None;
        }
        match event {
            ComponentEvent::Text(text) => {
                let text = normalize(text)?;
                self.edit(self.document.cursor..self.document.cursor, &text)?;
            }
            ComponentEvent::Paste(text) => self.paste(text)?,
            ComponentEvent::Key(event) if event.kind != KeyKind::Release => {
                let key = event.base_layout_key.unwrap_or(event.key);
                let modifiers = event.modifiers;
                if modifiers.super_key {
                    return Ok(EventResponse::default());
                }
                if !self.completions.is_empty()
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.shift
                {
                    match key {
                        Key::Up => {
                            self.selected_completion =
                                (self.selected_completion + self.completions.len() - 1)
                                    % self.completions.len()
                        }
                        Key::Down | Key::Tab => {
                            self.selected_completion =
                                (self.selected_completion + 1) % self.completions.len()
                        }
                        Key::Enter => self.accept_completion()?,
                        Key::Escape => self.completions.clear(),
                        _ => {
                            self.completions.clear();
                            return self.handle_event(&ComponentEvent::Key(*event), _host);
                        }
                    }
                } else if modifiers.control && !modifiers.alt {
                    match key {
                        Key::Character('z') => {
                            if let Some(document) = self.undo.pop_back() {
                                self.document = document;
                                self.history_index = None;
                                self.draft = None;
                                self.changed();
                            }
                        }
                        Key::Character('a') => {
                            self.document.cursor = self.document.text[..self.document.cursor]
                                .rfind('\n')
                                .map_or(0, |index| index + 1);
                            self.preferred_column = None;
                        }
                        Key::Character('e') => {
                            self.document.cursor = self.document.text[self.document.cursor..]
                                .find('\n')
                                .map_or(self.document.text.len(), |index| {
                                    self.document.cursor + index
                                });
                            self.preferred_column = None;
                        }
                        Key::Character('u') => {
                            let start = self.document.text[..self.document.cursor]
                                .rfind('\n')
                                .map_or(0, |index| index + 1);
                            self.kill(start..self.document.cursor, true)?;
                        }
                        Key::Character('k') => {
                            let end = self.document.text[self.document.cursor..]
                                .find('\n')
                                .map_or(self.document.text.len(), |index| {
                                    self.document.cursor + index
                                });
                            self.kill(self.document.cursor..end, false)?;
                        }
                        Key::Character('w') => {
                            self.kill(self.word_left()..self.document.cursor, true)?
                        }
                        Key::Character('y') => self.yank(false)?,
                        Key::Left => self.document.cursor = self.word_left(),
                        Key::Right => self.document.cursor = self.word_right(),
                        Key::Enter => self.insert_text("\n")?,
                        _ => return Ok(EventResponse::default()),
                    }
                } else if modifiers.alt && !modifiers.control {
                    match key {
                        Key::Character('b') | Key::Left => self.document.cursor = self.word_left(),
                        Key::Character('f') | Key::Right => {
                            self.document.cursor = self.word_right()
                        }
                        Key::Character('d') | Key::Delete => {
                            self.kill(self.document.cursor..self.word_right(), false)?
                        }
                        Key::Backspace => {
                            self.kill(self.word_left()..self.document.cursor, true)?
                        }
                        Key::Character('y') => self.yank(true)?,
                        Key::Enter => self.insert_text("\n")?,
                        Key::Up => {
                            self.navigate_history(true);
                        }
                        Key::Down => {
                            self.navigate_history(false);
                        }
                        _ => return Ok(EventResponse::default()),
                    }
                } else if !modifiers.control && !modifiers.alt {
                    match key {
                        Key::Enter if modifiers.shift => self.insert_text("\n")?,
                        Key::Enter => self.submit()?,
                        Key::Tab if !modifiers.shift => {
                            if !self.complete(true)? {
                                return Ok(EventResponse::default());
                            }
                        }
                        Key::Left => {
                            self.document.cursor = self.previous();
                            self.preferred_column = None;
                        }
                        Key::Right => {
                            self.document.cursor = self.next();
                            self.preferred_column = None;
                        }
                        Key::Up => self.move_vertical(-1),
                        Key::Down => self.move_vertical(1),
                        Key::PageUp => self.move_vertical(-i32::from(self.max_height.max(1))),
                        Key::PageDown => self.move_vertical(i32::from(self.max_height.max(1))),
                        Key::Home => {
                            self.document.cursor = self.document.text[..self.document.cursor]
                                .rfind('\n')
                                .map_or(0, |index| index + 1);
                            self.preferred_column = None;
                        }
                        Key::End => {
                            self.document.cursor = self.document.text[self.document.cursor..]
                                .find('\n')
                                .map_or(self.document.text.len(), |index| {
                                    self.document.cursor + index
                                });
                            self.preferred_column = None;
                        }
                        Key::Backspace => self.edit(self.previous()..self.document.cursor, "")?,
                        Key::Delete => self.edit(self.document.cursor..self.next(), "")?,
                        _ => return Ok(EventResponse::default()),
                    }
                } else {
                    return Ok(EventResponse::default());
                }
            }
            ComponentEvent::Pointer(event)
                if event.kind == PointerKind::Press
                    && event.button == Some(MouseButton::Left)
                    && event.row >= 0 =>
            {
                let row = event.row as usize;
                if row >= self.menu_row && row < self.menu_row + self.menu_visible {
                    self.selected_completion = self.menu_start + row - self.menu_row;
                    self.accept_completion()?;
                } else if row < self.menu_row {
                    if let Some(row) = self.rows.get(self.scroll_top + row) {
                        self.document.cursor = Self::at_column(row, event.column.max(0) as usize);
                    }
                    self.completions.clear();
                    self.preferred_column = None;
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
        if self.auto_complete && matches!(event, ComponentEvent::Text(_) | ComponentEvent::Paste(_))
        {
            self.complete(false)?;
        }
        Ok(EventResponse {
            handled: true,
            redraw: true,
            ..EventResponse::default()
        })
    }
    fn save_state(&self) -> ComponentResult<Option<StateSnapshot>> {
        let mut payload = Vec::new();
        write_number(&mut payload, self.document.cursor as u64);
        write_number(&mut payload, self.document.paste_id);
        write_string(&mut payload, &self.document.text);
        write_number(&mut payload, self.document.pastes.len() as u64);
        for paste in &self.document.pastes {
            write_number(&mut payload, paste.id);
            write_number(&mut payload, paste.range.start as u64);
            write_number(&mut payload, paste.range.end as u64);
            write_string(&mut payload, &paste.text);
        }
        Ok(Some(StateSnapshot {
            schema_version: 1,
            encoding: "application/x-bi-editor".into(),
            payload,
        }))
    }
    fn restore_state(&mut self, state: &StateSnapshot) -> ComponentResult<()> {
        if state.schema_version != 1 {
            return Err(ComponentError::UnsupportedState {
                version: state.schema_version,
            });
        }
        if state.encoding != "application/x-bi-editor" {
            return Err(invalid_state());
        }
        let mut bytes = state.payload.as_slice();
        let cursor = usize::try_from(read_number(&mut bytes)?).map_err(|_| invalid_state())?;
        let paste_id = read_number(&mut bytes)?;
        let text = read_string(&mut bytes)?;
        validate_text(&text, true)?;
        let count = read_number(&mut bytes)?;
        if count > (bytes.len() / 32) as u64 {
            return Err(invalid_state());
        }
        let mut pastes = Vec::new();
        let mut ids = std::collections::HashSet::new();
        let mut end = 0;
        for _ in 0..count {
            let id = read_number(&mut bytes)?;
            let start = usize::try_from(read_number(&mut bytes)?).map_err(|_| invalid_state())?;
            let stop = usize::try_from(read_number(&mut bytes)?).map_err(|_| invalid_state())?;
            let pasted = read_string(&mut bytes)?;
            validate_text(&pasted, true)?;
            let marker = format!("[paste #{id} +{} lines]", pasted.split('\n').count());
            if id == 0
                || id > paste_id
                || !ids.insert(id)
                || start < end
                || text.get(start..stop) != Some(marker.as_str())
            {
                return Err(invalid_state());
            }
            end = stop;
            pastes.push(Paste {
                id,
                range: start..stop,
                text: pasted,
            });
        }
        let document = Document {
            text,
            cursor,
            pastes,
            paste_id,
        };
        if !bytes.is_empty() || !segment_boundaries(&document).contains(&cursor) {
            return Err(invalid_state());
        }
        self.document = document;
        self.undo.clear();
        self.last_insert = None;
        self.last_kill = false;
        self.yank = None;
        self.typing = false;
        self.history_index = None;
        self.draft = None;
        self.scroll_top = 0;
        self.changed();
        Ok(())
    }
}

fn normalize(text: &str) -> ComponentResult<String> {
    let text = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ");
    validate_text(&text, true)?;
    Ok(text)
}

fn segment_boundaries(document: &Document) -> Vec<usize> {
    let mut boundaries = Vec::new();
    let mut start = 0;
    for paste in &document.pastes {
        boundaries.extend(
            document.text[start..paste.range.start]
                .grapheme_indices(true)
                .map(|(index, _)| start + index),
        );
        boundaries.push(paste.range.start);
        boundaries.push(paste.range.end);
        start = paste.range.end;
    }
    boundaries.extend(
        document.text[start..]
            .grapheme_indices(true)
            .map(|(index, _)| start + index),
    );
    boundaries.push(document.text.len());
    boundaries.dedup();
    boundaries
}
fn invalid_state() -> ComponentError {
    ComponentError::InvalidContent {
        reason: "invalid Editor state".into(),
    }
}
fn write_number(bytes: &mut Vec<u8>, number: u64) {
    bytes.extend_from_slice(&number.to_le_bytes());
}
fn write_string(bytes: &mut Vec<u8>, text: &str) {
    write_number(bytes, text.len() as u64);
    bytes.extend_from_slice(text.as_bytes());
}
fn read_number(bytes: &mut &[u8]) -> ComponentResult<u64> {
    if bytes.len() < 8 {
        return Err(invalid_state());
    }
    let number = u64::from_le_bytes(bytes[..8].try_into().map_err(|_| invalid_state())?);
    *bytes = &bytes[8..];
    Ok(number)
}
fn read_string(bytes: &mut &[u8]) -> ComponentResult<String> {
    let length = usize::try_from(read_number(bytes)?).map_err(|_| invalid_state())?;
    if length > bytes.len() {
        return Err(invalid_state());
    }
    let text = std::str::from_utf8(&bytes[..length])
        .map_err(|_| invalid_state())?
        .to_owned();
    *bytes = &bytes[length..];
    Ok(text)
}
