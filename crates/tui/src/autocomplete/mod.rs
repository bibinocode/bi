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
    fuzzy: bool,
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
        Ok(Self {
            commands,
            fuzzy: false,
        })
    }
    pub fn with_fuzzy(mut self, enabled: bool) -> Self {
        self.fuzzy = enabled;
        self
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
        let mut matches: Vec<_> = self
            .commands
            .iter()
            .filter_map(|command| {
                let score = if self.fuzzy {
                    crate::utils::fuzzy::fuzzy_score(&prefix, &command.name)?
                } else if command.name.to_lowercase().starts_with(&prefix) {
                    0
                } else {
                    return None;
                };
                Some((
                    score,
                    Completion {
                        label: format!("/{}", command.name),
                        replacement: format!("/{}", command.name),
                        description: command.description.clone(),
                        range: start..end,
                    },
                ))
            })
            .collect();
        matches.sort_by_key(|item| std::cmp::Reverse(item.0));
        Ok(matches
            .into_iter()
            .map(|(_, completion)| completion)
            .collect())
    }
}

/// 按配置顺序合并补全源；重复替换项只保留第一个。
#[derive(Default)]
pub struct CombinedAutocomplete {
    providers: Vec<Box<dyn AutocompleteProvider>>,
}
impl CombinedAutocomplete {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_provider(mut self, provider: impl AutocompleteProvider + 'static) -> Self {
        self.providers.push(Box::new(provider));
        self
    }
}
impl AutocompleteProvider for CombinedAutocomplete {
    fn suggestions(&self, text: &str, cursor: usize) -> ComponentResult<Vec<Completion>> {
        let mut result = Vec::new();
        for provider in &self.providers {
            for item in provider.suggestions(text, cursor)? {
                if !result.iter().any(|old: &Completion| {
                    old.range == item.range && old.replacement == item.replacement
                }) {
                    result.push(item);
                }
            }
        }
        Ok(result)
    }
}

/// 文件系统补全；在输入事件中枚举目录，不在布局阶段执行 IO。
pub struct PathAutocomplete {
    directory: std::path::PathBuf,
    fuzzy: bool,
}
impl PathAutocomplete {
    pub fn new(directory: impl Into<std::path::PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            fuzzy: false,
        }
    }
    pub fn with_fuzzy(mut self, enabled: bool) -> Self {
        self.fuzzy = enabled;
        self
    }
}
impl AutocompleteProvider for PathAutocomplete {
    fn suggestions(&self, text: &str, cursor: usize) -> ComponentResult<Vec<Completion>> {
        if cursor > text.len() || !text.is_char_boundary(cursor) {
            return Err(ComponentError::InvalidContent {
                reason: "invalid completion cursor".into(),
            });
        }
        let mut start = 0;
        let mut quote = None;
        for (index, ch) in text[..cursor].char_indices() {
            if ch == '"' || ch == '\'' {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                }
            }
            if ch.is_whitespace() && quote.is_none() {
                start = index + ch.len_utf8();
            }
        }
        let token = &text[start..cursor];
        let quoted = token.chars().next().filter(|c| matches!(c, '"' | '\''));
        let token = token.trim_matches(['"', '\'']);
        if token.is_empty() {
            return Ok(Vec::new());
        }
        let split = token.rfind(['/', '\\']).map_or(0, |index| index + 1);
        let parent = &token[..split];
        let prefix = &token[split..];
        let directory = if let Some(rest) = parent
            .strip_prefix("~/")
            .or_else(|| parent.strip_prefix("~\\"))
        {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(std::path::PathBuf::from)
                .map(|home| home.join(rest))
                .unwrap_or_else(|| self.directory.join(parent))
        } else {
            self.directory.join(parent)
        };
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(Vec::new());
            }
            Err(error) => {
                return Err(ComponentError::OperationFailed {
                    message: format!("path completion: {error}"),
                });
            }
        };
        let end = text[cursor..]
            .char_indices()
            .find(|(_, ch)| {
                if let Some(q) = quoted {
                    *ch == q
                } else {
                    ch.is_whitespace()
                }
            })
            .map_or(text.len(), |(i, ch)| {
                cursor + i + if quoted.is_some() { ch.len_utf8() } else { 0 }
            });
        let mut matches = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| ComponentError::OperationFailed {
                message: error.to_string(),
            })?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.chars().any(char::is_control) || name.contains(['"', '\'']) {
                continue;
            }
            let score = if self.fuzzy {
                crate::utils::fuzzy::fuzzy_score(prefix, &name)
            } else {
                name.to_lowercase()
                    .starts_with(&prefix.to_lowercase())
                    .then_some(0)
            };
            let Some(score) = score else {
                continue;
            };
            let is_dir = entry.path().is_dir();
            let suffix = if is_dir {
                if parent.contains('\\') { "\\" } else { "/" }
            } else {
                ""
            };
            let replacement = format!("{parent}{name}{suffix}");
            let replacement = if quoted.is_some() || replacement.chars().any(char::is_whitespace) {
                let q = quoted.unwrap_or('"');
                format!("{q}{replacement}{q}")
            } else {
                replacement
            };
            matches.push((
                score,
                Completion {
                    label: format!("{name}{suffix}"),
                    replacement,
                    description: Some(if is_dir { "目录" } else { "文件" }.into()),
                    range: start..end,
                },
            ));
        }
        matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.label.cmp(&b.1.label)));
        Ok(matches
            .into_iter()
            .take(256)
            .map(|(_, item)| item)
            .collect())
    }
}
