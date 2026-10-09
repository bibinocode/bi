#![allow(dead_code)]

use bi_tui::{
    component::{Component, ComponentNode},
    protocol::{
        Capabilities, ComponentHandle, ComponentHost, ComponentId, ComponentResult, Key, KeyEvent,
        KeyKind, LayoutContext, MessageSender, Modifiers, ResourceCleanup, ResourceId, ScreenMode,
        Size,
    },
};
use std::{collections::BTreeMap, sync::mpsc};

pub fn context(width: u16, height: Option<u16>) -> LayoutContext {
    LayoutContext {
        width,
        terminal_size: Size {
            width: 80,
            height: 24,
        },
        available_height: height,
        screen_mode: ScreenMode::Alternate,
        capabilities: Capabilities::default(),
        focused: true,
    }
}

pub fn key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::default(),
        kind: KeyKind::Press,
        base_layout_key: None,
    }
}
pub fn node(id: u64, component: impl Component + 'static) -> ComponentNode {
    ComponentNode::new(
        ComponentHandle::initial(ComponentId(id)),
        Box::new(component),
    )
}

pub struct Host {
    sender: MessageSender,
    cleanups: BTreeMap<u64, ResourceCleanup>,
    next: u64,
}
impl Default for Host {
    fn default() -> Self {
        let (sender, _) = mpsc::channel();
        Self {
            sender: MessageSender::new(ComponentHandle::initial(ComponentId(1000)), sender),
            cleanups: BTreeMap::new(),
            next: 1,
        }
    }
}
impl ComponentHost for Host {
    fn handle(&self) -> ComponentHandle {
        self.sender.source()
    }
    fn request_redraw(&mut self) -> ComponentResult<()> {
        Ok(())
    }
    fn message_sender(&self) -> MessageSender {
        self.sender.clone()
    }
    fn register_cleanup(&mut self, cleanup: ResourceCleanup) -> ComponentResult<ResourceId> {
        let id = self.next;
        self.next += 1;
        self.cleanups.insert(id, cleanup);
        Ok(ResourceId(id))
    }
    fn release_resource(&mut self, id: ResourceId) -> ComponentResult<()> {
        if let Some(cleanup) = self.cleanups.remove(&id.0) {
            cleanup();
        }
        Ok(())
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        for cleanup in std::mem::take(&mut self.cleanups).into_values() {
            cleanup();
        }
    }
}
