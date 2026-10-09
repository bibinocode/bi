//!Crossterm 事件适配器

// 先发送 ComponentEvent::Key，让快捷键有机会处理。
// 如果按键没有被消费，再发送 ComponentEvent::Text("a")
// 所以适配器先保留两部分信息，不会直接同时发送两个组件事件

use std::io;

use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent as CrosstermKeyEvent, KeyEventKind,
    KeyModifiers, MouseButton as CrosstermMouseButton, MouseEventKind,
};

use crate::protocol::{
    ComponentEvent, Key, KeyEvent, KeyKind, Modifiers, MouseButton, PointerEvent, PointerKind, Size,
};

/// 终端适配后的事件。
///
/// 窗口尺寸和窗口焦点属于终端事件，
/// 不直接等同于某个组件的事件。
#[derive(Debug)]
pub enum TerminalEvent {
    Key {
        event: KeyEvent,

        /// 按键未被消费时，运行时可以提交这段文本。
        text: Option<String>,
    },

    Component(ComponentEvent),

    Resize(Size),

    WindowFocusChanged(bool),

    /// 当前组件协议无法表达的事件，保留原始信息。
    Unsupported(CrosstermEvent),
}

pub fn read_event() -> io::Result<TerminalEvent> {
    event::read().map(normalize_event)
}

pub fn normalize_event(event: CrosstermEvent) -> TerminalEvent {
    match event {
        CrosstermEvent::Key(key) => match normalize_key(key) {
            Some((event, text)) => TerminalEvent::Key { event, text },
            None => TerminalEvent::Unsupported(CrosstermEvent::Key(key)),
        },

        CrosstermEvent::Paste(text) => TerminalEvent::Component(ComponentEvent::Paste(text)),

        CrosstermEvent::Resize(width, height) => TerminalEvent::Resize(Size { width, height }),

        CrosstermEvent::FocusGained => TerminalEvent::WindowFocusChanged(true),

        CrosstermEvent::FocusLost => TerminalEvent::WindowFocusChanged(false),

        CrosstermEvent::Mouse(mouse) => {
            let Some(modifiers) = normalize_modifiers(mouse.modifiers) else {
                return TerminalEvent::Unsupported(CrosstermEvent::Mouse(mouse));
            };

            let (kind, button) = match mouse.kind {
                MouseEventKind::Down(button) => {
                    (PointerKind::Press, Some(normalize_mouse_button(button)))
                }
                MouseEventKind::Up(button) => {
                    (PointerKind::Release, Some(normalize_mouse_button(button)))
                }
                MouseEventKind::Drag(button) => {
                    (PointerKind::Drag, Some(normalize_mouse_button(button)))
                }
                MouseEventKind::Moved => (PointerKind::Move, None),
                MouseEventKind::ScrollUp => (
                    PointerKind::Scroll {
                        columns: 0,
                        rows: -1,
                    },
                    None,
                ),
                MouseEventKind::ScrollDown => (
                    PointerKind::Scroll {
                        columns: 0,
                        rows: 1,
                    },
                    None,
                ),
                MouseEventKind::ScrollLeft => (
                    PointerKind::Scroll {
                        columns: -1,
                        rows: 0,
                    },
                    None,
                ),
                MouseEventKind::ScrollRight => (
                    PointerKind::Scroll {
                        columns: 1,
                        rows: 0,
                    },
                    None,
                ),
            };

            TerminalEvent::Component(ComponentEvent::Pointer(PointerEvent {
                column: i32::from(mouse.column),
                row: i64::from(mouse.row),
                kind,
                button,
                modifiers,
                click_count: 0,
            }))
        }
    }
}

fn normalize_key(raw: CrosstermKeyEvent) -> Option<(KeyEvent, Option<String>)> {
    let mut modifiers = normalize_modifiers(raw.modifiers)?;

    let key = match raw.code {
        KeyCode::Char(character) => Key::Character(character),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Escape,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => {
            modifiers.shift = true;
            Key::Tab
        }
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::F(number) => Key::Function(number),

        // Insert、媒体键等尚未定义在组件协议中。
        _ => return None,
    };

    let kind = match raw.kind {
        KeyEventKind::Press => KeyKind::Press,
        KeyEventKind::Repeat => KeyKind::Repeat,
        KeyEventKind::Release => KeyKind::Release,
    };

    let text = match key {
        Key::Character(character)
            if kind != KeyKind::Release
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.super_key
                && !character.is_control() =>
        {
            Some(character.to_string())
        }
        _ => None,
    };

    Some((
        KeyEvent {
            key,
            modifiers,
            kind,

            // 普通 Crossterm KeyEvent 没有提供基础布局按键。
            base_layout_key: None,
        },
        text,
    ))
}

fn normalize_modifiers(raw: KeyModifiers) -> Option<Modifiers> {
    let supported =
        KeyModifiers::SHIFT | KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER;

    // 不把协议无法表达的修饰键悄悄丢弃。
    if !raw.difference(supported).is_empty() {
        return None;
    }

    Some(Modifiers {
        shift: raw.contains(KeyModifiers::SHIFT),
        control: raw.contains(KeyModifiers::CONTROL),
        alt: raw.contains(KeyModifiers::ALT),
        super_key: raw.contains(KeyModifiers::SUPER),
    })
}

fn normalize_mouse_button(button: CrosstermMouseButton) -> MouseButton {
    match button {
        CrosstermMouseButton::Left => MouseButton::Left,
        CrosstermMouseButton::Middle => MouseButton::Middle,
        CrosstermMouseButton::Right => MouseButton::Right,
    }
}
