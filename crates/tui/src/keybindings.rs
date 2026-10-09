use crate::{
    keys::KeyChord,
    protocol::{ComponentResult, KeyEvent},
};
use std::collections::HashMap;
/// 会话级快捷键重映射；目标使用组件现有的规范按键，None 表示禁用该键。
#[derive(Default, Clone)]
pub struct Keybindings {
    bindings: HashMap<KeyChord, Option<KeyChord>>,
}
impl Keybindings {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn bind(&mut self, from: &str, to: &str) -> ComponentResult<()> {
        self.bindings
            .insert(KeyChord::parse(from)?, Some(KeyChord::parse(to)?));
        Ok(())
    }
    pub fn disable(&mut self, from: &str) -> ComponentResult<()> {
        self.bindings.insert(KeyChord::parse(from)?, None);
        Ok(())
    }
    pub fn translate(&self, event: KeyEvent) -> Option<(KeyEvent, bool)> {
        let chord = KeyChord::new(event.base_layout_key.unwrap_or(event.key), event.modifiers);
        match self.bindings.get(&chord) {
            Some(Some(target)) => Some((
                KeyEvent {
                    key: target.key,
                    modifiers: target.modifiers,
                    base_layout_key: None,
                    ..event
                },
                true,
            )),
            Some(None) => None,
            None => Some((event, false)),
        }
    }
}
