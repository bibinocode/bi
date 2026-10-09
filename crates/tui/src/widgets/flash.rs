use crate::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{
        ComponentError, ComponentEvent, ComponentHost, ComponentResult, EventResponse,
        LayoutContext, Line, Span, Style,
    },
    utils::text::truncate_text,
};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// 消息驱动的临时提示。向组件发送 bi.flash 消息，payload 为 UTF-8 文本。
pub struct Flash {
    duration: Duration,
    entries: Vec<(String, Instant)>,
}
impl Flash {
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            entries: Vec::new(),
        }
    }
}
impl Default for Flash {
    fn default() -> Self {
        Self::new(Duration::from_secs(1))
    }
}
impl Component for Flash {
    fn mount(&mut self, host: &mut dyn ComponentHost) -> ComponentResult<()> {
        let sender = host.message_sender();
        let target = host.handle();
        let (cancel, receiver) = mpsc::channel();
        let timer = thread::Builder::new()
            .name("bi-flash".into())
            .spawn(move || {
                while let Err(mpsc::RecvTimeoutError::Timeout) =
                    receiver.recv_timeout(Duration::from_millis(25))
                {
                    if sender.post(target, "bi.flash.tick", Vec::new()).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| ComponentError::OperationFailed {
                message: e.to_string(),
            })?;
        host.register_cleanup(Box::new(move || {
            let _ = cancel.send(());
            let _ = timer.join();
        }))?;
        Ok(())
    }
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        if !children.is_empty() {
            return Err(ComponentError::InvalidLayout {
                reason: "Flash does not accept children".into(),
            });
        }
        if context.width == 0 {
            return Ok(LayoutSnapshot::empty(0));
        }
        let lines = self
            .entries
            .iter()
            .take(context.available_height.map_or(usize::MAX, usize::from))
            .map(|(text, _)| {
                Ok(Line {
                    spans: vec![Span::styled(
                        truncate_text(text, context.width, "…")?,
                        Style {
                            reversed: true,
                            ..Style::default()
                        },
                    )],
                })
            })
            .collect::<ComponentResult<Vec<_>>>()?;
        Ok(LayoutSnapshot::from_lines(context.width, lines))
    }
    fn handle_event(
        &mut self,
        event: &ComponentEvent,
        _host: &mut dyn ComponentHost,
    ) -> ComponentResult<EventResponse> {
        if let ComponentEvent::Message { topic, payload } = event {
            if topic == "bi.flash" {
                let text =
                    std::str::from_utf8(payload).map_err(|e| ComponentError::InvalidContent {
                        reason: e.to_string(),
                    })?;
                crate::utils::text::display_width(text)?;
                self.entries.push((text.into(), Instant::now()));
                if self.entries.len() > 100 {
                    self.entries.remove(0);
                }
                return Ok(EventResponse {
                    handled: true,
                    redraw: true,
                    ..EventResponse::default()
                });
            }
            if topic == "bi.flash.tick" {
                let old = self.entries.len();
                self.entries.retain(|(_, at)| at.elapsed() < self.duration);
                return Ok(EventResponse {
                    handled: true,
                    redraw: self.entries.len() != old,
                    ..EventResponse::default()
                });
            }
        }
        Ok(EventResponse::default())
    }
}
