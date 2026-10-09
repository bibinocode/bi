mod application;
mod event;
mod output;
mod session;
pub use output::TerminalOutput;

pub use application::{
    DrawnLayout, LoopControl, RunOptions, TuiError, dispatch_terminal_event, draw_runtime, run,
};
pub use event::{TerminalEvent, normalize_event, read_event};
pub use session::{InputModes, TerminalSession};

/// 根据已知环境标识推断能力；不发送探测查询，可由 RunOptions 显式覆盖。
pub fn detect_capabilities() -> crate::protocol::Capabilities {
    use crate::protocol::{Capabilities, ImageProtocol};
    let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    let term = std::env::var("TERM").unwrap_or_default();
    let kitty = std::env::var_os("KITTY_WINDOW_ID").is_some() || term.contains("kitty");
    let iterm = program == "iTerm.app";
    Capabilities {
        true_color: std::env::var_os("NO_COLOR").is_none()
            && (std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit")
                || kitty
                || iterm
                || std::env::var_os("WT_SESSION").is_some()),
        hyperlinks: kitty
            || iterm
            || std::env::var_os("WT_SESSION").is_some()
            || program == "WezTerm"
            || program == "vscode",
        images: if kitty {
            Some(ImageProtocol::Kitty)
        } else if iterm {
            Some(ImageProtocol::Iterm2)
        } else {
            None
        },
    }
}
