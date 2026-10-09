use std::collections::VecDeque;
/// Emacs 风格删除缓冲区，保存最多 100 条，不等同于系统剪贴板。
#[derive(Default)]
pub struct KillRing {
    entries: VecDeque<String>,
}
impl KillRing {
    pub fn push(&mut self, text: String, backward: bool, accumulate: bool) {
        if text.is_empty() {
            return;
        }
        if accumulate && let Some(last) = self.entries.back_mut() {
            if backward {
                last.insert_str(0, &text);
            } else {
                last.push_str(&text);
            }
        } else {
            self.entries.push_back(text);
            if self.entries.len() > 100 {
                self.entries.pop_front();
            }
        }
    }
    pub fn peek(&self) -> Option<&str> {
        self.entries.back().map(String::as_str)
    }
    pub fn rotate(&mut self) {
        if self.entries.len() > 1
            && let Some(last) = self.entries.pop_back()
        {
            self.entries.push_front(last);
        }
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
