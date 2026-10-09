use crate::protocol::{ComponentError, ComponentResult, Key, KeyEvent, Modifiers};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: Key,
    pub modifiers: Modifiers,
}
impl KeyChord {
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }
    /// 例如 ctrl+shift+left、alt+y、enter、f5。加号键可写 plus。
    pub fn parse(value: &str) -> ComponentResult<Self> {
        let mut modifiers = Modifiers::default();
        let mut key = None;
        for part in value.to_lowercase().split('+') {
            match part {
                "ctrl" | "control" => modifiers.control = true,
                "alt" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "super" => modifiers.super_key = true,
                _ => {
                    if key.is_some() {
                        return Err(ComponentError::InvalidContent {
                            reason: "key chord contains multiple keys".into(),
                        });
                    }
                    key = Some(match part {
                        "enter" => Key::Enter,
                        "escape" | "esc" => Key::Escape,
                        "tab" => Key::Tab,
                        "backspace" => Key::Backspace,
                        "delete" => Key::Delete,
                        "left" => Key::Left,
                        "right" => Key::Right,
                        "up" => Key::Up,
                        "down" => Key::Down,
                        "home" => Key::Home,
                        "end" => Key::End,
                        "pageup" => Key::PageUp,
                        "pagedown" => Key::PageDown,
                        "space" => Key::Character(' '),
                        "plus" => Key::Character('+'),
                        part if part.starts_with('f') && part.len() > 1 => Key::Function(
                            part[1..]
                                .parse()
                                .ok()
                                .filter(|n| *n > 0 && *n <= 24)
                                .ok_or_else(|| ComponentError::InvalidContent {
                                    reason: "invalid function key".into(),
                                })?,
                        ),
                        part if part.chars().count() == 1 => {
                            Key::Character(part.chars().next().unwrap())
                        }
                        _ => {
                            return Err(ComponentError::InvalidContent {
                                reason: format!("unknown key: {part}"),
                            });
                        }
                    });
                }
            }
        }
        Ok(Self {
            key: key.ok_or_else(|| ComponentError::InvalidContent {
                reason: "key chord has no key".into(),
            })?,
            modifiers,
        })
    }
    pub fn matches(self, event: &KeyEvent) -> bool {
        self.key == event.base_layout_key.unwrap_or(event.key) && self.modifiers == event.modifiers
    }
}
