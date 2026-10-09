use std::{io, io::Write, time::Duration};

use ratatui::{Terminal, backend::Backend};
use thiserror::Error;

use super::{TerminalEvent, TerminalSession, read_event};
use crate::{
    component::LayoutNode,
    protocol::{
        Capabilities, ClipRect, ComponentError, ComponentEvent, Key, KeyKind, LayoutContext,
        Offset, ScreenMode, Size,
    },
    render::{layout_cursor, paint_layout, paint_layout_with_images},
    runtime::{PointerTracker, Runtime},
};

#[derive(Debug, Error)]
pub enum TuiError {
    #[error(transparent)]
    Component(#[from] ComponentError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("terminal backend failed: {0}")]
    Backend(String),
    #[error("{primary}; component shutdown also failed: {cleanup}")]
    Shutdown {
        primary: String,
        cleanup: ComponentError,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    /// 消息轮询间隔，必须大于零。
    pub poll_interval: Duration,
    /// 每轮最多处理的异步消息数，必须大于零。
    pub message_budget: usize,
    /// 未被组件消费的 Escape 是否退出；Ctrl+C 始终退出。
    pub exit_on_escape: bool,
    pub capabilities: Capabilities,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(16),
            message_budget: 64,
            exit_on_escape: true,
            capabilities: super::detect_capabilities(),
        }
    }
}

/// 已提交的一帧及其鼠标命中坐标系。
pub struct DrawnLayout {
    pub root: Option<LayoutNode>,
    pub offset: Offset,
    pub viewport: ClipRect,
    pub buffer: ratatui::buffer::Buffer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopControl {
    Continue,
    Exit,
}

/// 处理一个已标准化的事件，可用于自定义事件循环或无终端测试。
pub fn dispatch_terminal_event(
    runtime: &mut Runtime,
    event: TerminalEvent,
    drawn: Option<&DrawnLayout>,
    pointer: &mut PointerTracker,
    options: RunOptions,
) -> Result<LoopControl, ComponentError> {
    match event {
        TerminalEvent::Key { event, text } => {
            if event.kind != KeyKind::Release
                && event.key == Key::Character('c')
                && event.modifiers.control
                && !event.modifiers.alt
                && !event.modifiers.super_key
            {
                return Ok(LoopControl::Exit);
            }
            let handled = runtime.dispatch_key(event, text)?;
            if !handled
                && event.key == Key::Tab
                && event.kind != KeyKind::Release
                && !event.modifiers.control
                && !event.modifiers.alt
                && !event.modifiers.super_key
            {
                runtime.focus_next(event.modifiers.shift)?;
            }
            if !handled
                && options.exit_on_escape
                && event.key == Key::Escape
                && event.kind != KeyKind::Release
            {
                return Ok(LoopControl::Exit);
            }
        }
        TerminalEvent::Component(ComponentEvent::Paste(text)) => {
            runtime.dispatch_paste(text)?;
        }
        TerminalEvent::Component(ComponentEvent::Pointer(event)) => {
            if let Some(DrawnLayout {
                root: Some(root),
                offset,
                viewport,
                ..
            }) = drawn
            {
                for event in pointer.process(event) {
                    runtime.dispatch_pointer(event, root, *offset, *viewport)?;
                }
            }
        }
        TerminalEvent::Resize(_) | TerminalEvent::WindowFocusChanged(_) => {
            pointer.reset();
            runtime.request_redraw();
        }
        _ => {}
    }
    Ok(LoopControl::Continue)
}

/// 绘制可在 TestBackend 中验证；IO 失败时保留重绘请求。
pub fn draw_runtime<B: Backend>(
    runtime: &mut Runtime,
    terminal: &mut Terminal<B>,
    screen_mode: ScreenMode,
    capabilities: Capabilities,
) -> Result<DrawnLayout, TuiError>
where
    B::Error: std::fmt::Display,
{
    let result = (|| {
        let size = terminal
            .size()
            .map_err(|error| TuiError::Backend(error.to_string()))?;
        let mut drawn = None;
        let mut render_result = Ok(());
        terminal
            .draw(|frame| {
                let area = frame.area();
                let offset = Offset {
                    column: i32::from(area.x),
                    row: i64::from(area.y),
                };
                let viewport = ClipRect {
                    column: offset.column,
                    row: offset.row,
                    width: area.width,
                    height: usize::from(area.height),
                };
                render_result = (|| {
                    let root = runtime.layout(&LayoutContext {
                        width: area.width,
                        terminal_size: Size {
                            width: size.width,
                            height: size.height,
                        },
                        available_height: Some(area.height),
                        screen_mode,
                        capabilities,
                        focused: false,
                    })?;
                    if let Some(root) = &root {
                        if capabilities.images.is_some() {
                            paint_layout_with_images(root, offset, frame.buffer_mut())?;
                        } else {
                            paint_layout(root, offset, frame.buffer_mut())?;
                        }
                        if let Some(cursor) =
                            layout_cursor(root, runtime.focused_handle(), offset, viewport)?
                        {
                            frame.set_cursor_position(cursor);
                        }
                    } else {
                        frame.buffer_mut().reset();
                    }
                    drawn = Some(DrawnLayout {
                        root,
                        offset,
                        viewport,
                        buffer: frame.buffer_mut().clone(),
                    });
                    Ok::<_, ComponentError>(())
                })();
            })
            .map_err(|error| TuiError::Backend(error.to_string()))?;
        render_result?;
        Ok(drawn.expect("draw callback completed"))
    })();
    if result.is_err() {
        runtime.request_redraw();
    }
    result
}

/// 运行组件树。退出或错误后卸载全部组件；终端会话由调用者 close 或 Drop 恢复。
pub fn run(
    runtime: &mut Runtime,
    session: &mut TerminalSession,
    options: RunOptions,
) -> Result<(), TuiError> {
    let result = if options.poll_interval.is_zero() || options.message_budget == 0 {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "poll interval and message budget must be greater than zero",
        )
        .into())
    } else {
        run_loop(runtime, session, options)
    };
    let shutdown = runtime.shutdown();
    match (result, shutdown) {
        (Err(error), Err(cleanup)) => Err(TuiError::Shutdown {
            primary: error.to_string(),
            cleanup,
        }),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), result) => result.map_err(Into::into),
    }
}

fn run_loop(
    runtime: &mut Runtime,
    session: &mut TerminalSession,
    options: RunOptions,
) -> Result<(), TuiError> {
    let mut drawn = None;
    let mut pointer = PointerTracker::default();
    let mut output = super::TerminalOutput::default();
    runtime.request_redraw();
    let result = (|| {
        loop {
            runtime.drain_messages(options.message_budget)?;
            if runtime.take_redraw_request() {
                let mode = session.screen_mode();
                if output.needs_clear() {
                    session
                        .terminal_mut()
                        .clear()
                        .map_err(|error| TuiError::Backend(error.to_string()))?;
                }
                // 同步更新包裹文本、链接与图片，失败时也尝试结束同步模式。
                let mut writer = io::stdout();
                writer.write_all(b"\x1b[?2026h")?;
                writer.flush()?;
                if let Some(maximum) = session.dynamic_max_height() {
                    let size = session.terminal_mut().size()?;
                    let natural = runtime
                        .layout(&LayoutContext {
                            width: size.width,
                            terminal_size: Size {
                                width: size.width,
                                height: size.height,
                            },
                            available_height: None,
                            screen_mode: mode,
                            capabilities: options.capabilities,
                            focused: false,
                        })?
                        .map_or(1, |root| root.snapshot.height.max(1));
                    session.resize_inline_height(
                        natural
                            .min(usize::from(maximum))
                            .min(usize::from(size.height.max(1))) as u16,
                    )?;
                }
                let frame =
                    draw_runtime(runtime, session.terminal_mut(), mode, options.capabilities)
                        .and_then(|frame| {
                            if let Some(root) = &frame.root {
                                output.write_frame(
                                    &mut writer,
                                    root,
                                    frame.offset,
                                    frame.viewport,
                                    &frame.buffer,
                                    options.capabilities,
                                )?;
                            } else {
                                output.clear(&mut writer)?;
                            }
                            Ok(frame)
                        });
                let end = writer
                    .write_all(b"\x1b[?2026l")
                    .and_then(|_| writer.flush());
                drawn = Some(frame?);
                end?;
            }
            if !crossterm::event::poll(options.poll_interval)? {
                continue;
            }
            if dispatch_terminal_event(
                runtime,
                read_event()?,
                drawn.as_ref(),
                &mut pointer,
                options,
            )? == LoopControl::Exit
            {
                return Ok(());
            }
        }
    })();
    let cleanup = io::stdout()
        .write_all(b"\x1b[?2026l")
        .and_then(|_| output.clear(&mut io::stdout()));
    result.and_then(|()| cleanup.map_err(Into::into))
}
