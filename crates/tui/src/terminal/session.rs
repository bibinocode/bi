use std::io::{self, stdout};

use crossterm::{
    Command,
    cursor::Show,
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture,
    },
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
        is_raw_mode_enabled,
    },
};
use ratatui::{DefaultTerminal, Terminal, TerminalOptions, Viewport, backend::CrosstermBackend};

use crate::protocol::ScreenMode;

/// 会话需要启用的终端输入模式。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InputModes {
    pub mouse_capture: bool,
    pub bracketed_paste: bool,
    pub focus_events: bool,
}

impl InputModes {
    pub const fn interactive() -> Self {
        Self {
            mouse_capture: true,
            bracketed_paste: true,
            focus_events: true,
        }
    }
}

/// 独占管理一次终端会话。
pub struct TerminalSession {
    terminal: DefaultTerminal,
    screen_mode: ScreenMode,
    state: TerminalState,
}

impl TerminalSession {
    pub fn alternate() -> io::Result<Self> {
        Self::alternate_with_input(InputModes::default())
    }

    pub fn alternate_with_input(input_modes: InputModes) -> io::Result<Self> {
        Self::open(ScreenMode::Alternate, Viewport::Fullscreen, input_modes)
    }

    pub fn main_screen(height: u16) -> io::Result<Self> {
        Self::main_screen_with_input(height, InputModes::default())
    }

    pub fn main_screen_with_input(height: u16, input_modes: InputModes) -> io::Result<Self> {
        if height == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "main-screen viewport height must be greater than zero",
            ));
        }

        Self::open(ScreenMode::Main, Viewport::Inline(height), input_modes)
    }

    pub fn screen_mode(&self) -> ScreenMode {
        self.screen_mode
    }

    pub fn terminal_mut(&mut self) -> &mut DefaultTerminal {
        &mut self.terminal
    }

    pub fn close(mut self) -> io::Result<()> {
        self.state.restore()
    }

    fn open(
        screen_mode: ScreenMode,
        viewport: Viewport,
        input_modes: InputModes,
    ) -> io::Result<Self> {
        let mut state = TerminalState::default();

        if !is_raw_mode_enabled()? {
            enable_raw_mode()?;
            state.raw_mode_owned = true;
        }

        if screen_mode == ScreenMode::Alternate {
            state.alternate_screen_owned = true;
            execute!(stdout(), EnterAlternateScreen)?;
        }

        // 在发送命令前记录恢复责任：
        // 即使命令部分写入后失败，也尝试发送对应的关闭命令。
        if input_modes.bracketed_paste {
            state.bracketed_paste_owned = true;
            execute!(stdout(), EnableBracketedPaste)?;
        }

        if input_modes.focus_events {
            state.focus_events_owned = true;
            execute!(stdout(), EnableFocusChange)?;
        }

        if input_modes.mouse_capture {
            state.mouse_capture_owned = true;
            execute!(stdout(), EnableMouseCapture)?;
        }

        state.restore_cursor = true;

        let backend = CrosstermBackend::new(stdout());

        let terminal = Terminal::with_options(backend, TerminalOptions { viewport })?;

        Ok(Self {
            terminal,
            screen_mode,
            state,
        })
    }
}

#[derive(Default)]
struct TerminalState {
    raw_mode_owned: bool,
    alternate_screen_owned: bool,
    restore_cursor: bool,
    bracketed_paste_owned: bool,
    focus_events_owned: bool,
    mouse_capture_owned: bool,
}

impl TerminalState {
    fn restore(&mut self) -> io::Result<()> {
        let mut first_error = None;

        // 一项失败不阻止其他项目恢复。
        restore_command(
            &mut self.mouse_capture_owned,
            DisableMouseCapture,
            &mut first_error,
        );

        restore_command(
            &mut self.focus_events_owned,
            DisableFocusChange,
            &mut first_error,
        );

        restore_command(
            &mut self.bracketed_paste_owned,
            DisableBracketedPaste,
            &mut first_error,
        );

        restore_command(&mut self.restore_cursor, Show, &mut first_error);

        restore_command(
            &mut self.alternate_screen_owned,
            LeaveAlternateScreen,
            &mut first_error,
        );

        if self.raw_mode_owned {
            match disable_raw_mode() {
                Ok(()) => self.raw_mode_owned = false,
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for TerminalState {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// 只有恢复成功，才清除该项恢复责任。
fn restore_command(pending: &mut bool, command: impl Command, first_error: &mut Option<io::Error>) {
    if !*pending {
        return;
    }

    match execute!(stdout(), command) {
        Ok(()) => *pending = false,
        Err(error) => {
            first_error.get_or_insert(error);
        }
    }
}
