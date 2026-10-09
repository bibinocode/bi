//! 编辑器补全接口；偏移使用 UTF-8 字节，而不是字符数或终端列号。
use crate::protocol::{ComponentError, ComponentResult};
use crate::utils::text::display_width;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub label: String,
    pub replacement: String,
    pub description: Option<String>,
    pub range: Range<usize>,
}

pub trait AutocompleteProvider {
    /// 在输入事件中调用，不能在 layout 中执行 IO。
    fn suggestions(&self, text: &str, cursor: usize) -> ComponentResult<Vec<Completion>>;
}

#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: String,
    pub description: Option<String>,
}
impl SlashCommand {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
        }
    }
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// 当前逻辑行开头的 /命令前缀补全；不修改光标后的参数。
pub struct CommandAutocomplete {
    commands: Vec<SlashCommand>,
}
impl CommandAutocomplete {
    pub fn new(commands: Vec<SlashCommand>) -> ComponentResult<Self> {
        for command in &commands {
            display_width(&command.name)?;
            if command.name.is_empty()
                || command.name.chars().any(|c| c.is_whitespace() || c == '/')
            {
                return Err(ComponentError::InvalidContent {
                    reason: "command name must be nonempty and contain no whitespace or slash"
                        .into(),
                });
            }
            if let Some(description) = &command.description {
                display_width(description)?;
            }
        }
        let mut names = std::collections::HashSet::new();
        if commands
            .iter()
            .any(|command| !names.insert(command.name.to_lowercase()))
        {
            return Err(ComponentError::InvalidContent {
                reason: "duplicate command name".into(),
            });
        }
        Ok(Self { commands })
    }
}
impl AutocompleteProvider for CommandAutocomplete {
    fn suggestions(&self, text: &str, cursor: usize) -> ComponentResult<Vec<Completion>> {
        if cursor > text.len() || !text.is_char_boundary(cursor) {
            return Err(ComponentError::InvalidContent {
                reason: "completion cursor is not a UTF-8 boundary".into(),
            });
        }
        let start = text[..cursor].rfind('\n').map_or(0, |index| index + 1);
        let Some(prefix) = text[start..cursor].strip_prefix('/') else {
            return Ok(Vec::new());
        };
        if prefix.chars().any(char::is_whitespace) {
            return Ok(Vec::new());
        }
        let end = text[cursor..]
            .find(char::is_whitespace)
            .map_or(text.len(), |offset| cursor + offset);
        let prefix = prefix.to_lowercase();
        Ok(self
            .commands
            .iter()
            .filter(|command| command.name.to_lowercase().starts_with(&prefix))
            .map(|command| Completion {
                label: format!("/{}", command.name),
                replacement: format!("/{}", command.name),
                description: command.description.clone(),
                range: start..end,
            })
            .collect())
    }
}
