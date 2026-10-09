use super::Loader;
use crate::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{
        ComponentEvent, ComponentHost, ComponentResult, EventResponse, Key, KeyKind, LayoutContext,
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// 可跨线程持有的取消信号；后台任务需要自行轮询 is_cancelled()。
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

pub struct CancellableLoader {
    loader: Loader,
    token: CancellationToken,
    notified: bool,
    on_abort: Option<Box<dyn FnMut()>>,
}
impl CancellableLoader {
    pub fn new(message: impl Into<String>) -> Self {
        Self::from_loader(Loader::new(message))
    }
    pub fn from_loader(loader: Loader) -> Self {
        Self {
            loader,
            token: CancellationToken::default(),
            notified: false,
            on_abort: None,
        }
    }
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }
    pub fn aborted(&self) -> bool {
        self.token.is_cancelled()
    }
    pub fn on_abort(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_abort = Some(Box::new(callback));
        self
    }
    fn finish_cancel(&mut self, host: &mut dyn ComponentHost) -> ComponentResult<()> {
        self.loader.stop(host)?;
        self.loader.set_message("已取消");
        if !self.notified {
            self.notified = true;
            if let Some(callback) = &mut self.on_abort {
                callback();
            }
        }
        Ok(())
    }
}
impl Component for CancellableLoader {
    fn focusable(&self) -> bool {
        true
    }
    fn mount(&mut self, host: &mut dyn ComponentHost) -> ComponentResult<()> {
        let token = self.token.clone();
        host.register_cleanup(Box::new(move || token.cancel()))?;
        if self.aborted() {
            self.finish_cancel(host)
        } else {
            self.loader.mount(host)
        }
    }
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        self.loader.layout(context, children)
    }
    fn handle_event(
        &mut self,
        event: &ComponentEvent,
        host: &mut dyn ComponentHost,
    ) -> ComponentResult<EventResponse> {
        let escape = matches!(event, ComponentEvent::Key(key) if key.key == Key::Escape && key.kind != KeyKind::Release && key.modifiers == Default::default());
        // 首次 Escape 消费为取消，第二次允许宿主执行退出行为。
        if escape && !self.aborted() {
            self.token.cancel();
        }
        if self.aborted() && !self.notified {
            self.finish_cancel(host)?;
            return Ok(EventResponse {
                handled: escape,
                redraw: true,
                ..EventResponse::default()
            });
        }
        self.loader.handle_event(event, host)
    }
}
