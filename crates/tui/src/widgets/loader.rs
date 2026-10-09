use super::Text;
use crate::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{
        ComponentError, ComponentEvent, ComponentHost, ComponentResult, EventResponse,
        LayoutContext, ResourceId,
    },
};
use std::{sync::mpsc, thread, time::Duration};

/// 动画由消息驱动，定时线程不直接访问组件；卸载时取消并等待线程结束。
pub struct Loader {
    message: String,
    frames: Vec<String>,
    frame: usize,
    interval: Duration,
    timer: Option<ResourceId>,
}

impl Loader {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            frames: ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
                .into_iter()
                .map(String::from)
                .collect(),
            frame: 0,
            interval: Duration::from_millis(80),
            timer: None,
        }
    }
    /// 空帧列表隐藏动画，单帧列表不启动定时线程。
    pub fn with_frames(mut self, frames: Vec<String>) -> Self {
        self.frames = frames;
        self.frame = 0;
        self
    }
    pub fn with_interval(mut self, interval: Duration) -> ComponentResult<Self> {
        if interval.is_zero() {
            return Err(ComponentError::InvalidContent {
                reason: "loader interval must be greater than zero".into(),
            });
        }
        self.interval = interval;
        Ok(self)
    }
    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
    }
    /// 停止当前实例的定时器；剩余已入队 tick 不再更新帧。
    pub fn stop(&mut self, host: &mut dyn ComponentHost) -> ComponentResult<()> {
        if let Some(timer) = self.timer.take() {
            host.release_resource(timer)?;
        }
        Ok(())
    }
}

impl Component for Loader {
    fn mount(&mut self, host: &mut dyn ComponentHost) -> ComponentResult<()> {
        if self.frames.len() <= 1 || self.timer.is_some() {
            return Ok(());
        }
        let sender = host.message_sender();
        let target = host.handle();
        let interval = self.interval;
        let (cancel, receiver) = mpsc::channel();
        let timer = thread::Builder::new()
            .name("bi-tui-loader".into())
            .spawn(move || {
                while let Err(mpsc::RecvTimeoutError::Timeout) = receiver.recv_timeout(interval) {
                    if sender.post(target, "bi.loader.tick", Vec::new()).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| ComponentError::OperationFailed {
                message: format!("cannot start loader timer: {error}"),
            })?;
        self.timer = Some(host.register_cleanup(Box::new(move || {
            let _ = cancel.send(());
            let _ = timer.join();
        }))?);
        Ok(())
    }
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        let indicator = self
            .frames
            .get(self.frame)
            .filter(|frame| !frame.is_empty())
            .map_or(String::new(), |frame| format!("{frame} "));
        Text::new(format!("{indicator}{}", self.message)).layout(context, children)
    }
    fn handle_event(
        &mut self,
        event: &ComponentEvent,
        _host: &mut dyn ComponentHost,
    ) -> ComponentResult<EventResponse> {
        if matches!(event, ComponentEvent::Message { topic, .. } if topic == "bi.loader.tick")
            && self.timer.is_some()
            && !self.frames.is_empty()
        {
            self.frame = (self.frame + 1) % self.frames.len();
            return Ok(EventResponse {
                handled: true,
                redraw: true,
                ..EventResponse::default()
            });
        }
        Ok(EventResponse::default())
    }
}
