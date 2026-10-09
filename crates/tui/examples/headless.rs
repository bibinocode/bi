//! 无真实终端的完整布局、输入路由与 Ratatui 绘制示例。
use bi_tui::{
    protocol::{Capabilities, Key, KeyEvent, KeyKind, Modifiers, ScreenMode},
    runtime::Runtime,
    terminal::draw_runtime,
    widgets::{BoxContainer, Input, Padding, Text},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{cell::RefCell, rc::Rc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let submitted = Rc::new(RefCell::new(String::new()));
    let output = Rc::clone(&submitted);
    let mut runtime = Runtime::new();
    let root = runtime.mount_root(Box::new(BoxContainer::new(Padding::all(1))))?;
    runtime.append_child(root, Box::new(Text::new("无终端交互验证")))?;
    let input = runtime.append_child(
        root,
        Box::new(Input::new().on_submit(move |value| *output.borrow_mut() = value.to_owned())),
    )?;
    runtime.set_focus(Some(input))?;
    runtime.dispatch_paste("中文 Rust".into())?;
    runtime.dispatch_key(
        KeyEvent {
            key: Key::Enter,
            modifiers: Modifiers::default(),
            kind: KeyKind::Press,
            base_layout_key: None,
        },
        None,
    )?;
    let mut terminal = Terminal::new(TestBackend::new(24, 6))?;
    let drawn = draw_runtime(
        &mut runtime,
        &mut terminal,
        ScreenMode::Alternate,
        Capabilities::default(),
    )?;
    for row in 0..6 {
        let mut line = String::new();
        let mut column = 0;
        while column < 24 {
            let symbol = terminal.backend().buffer()[(column, row)].symbol();
            line.push_str(symbol);
            column += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
        }
        println!("{row:02} |{line}|");
    }
    println!("提交回调：{}", submitted.borrow());
    println!(
        "焦点光标：{:?}",
        bi_tui::render::layout_cursor(
            drawn.root.as_ref().unwrap(),
            Some(input),
            drawn.offset,
            drawn.viewport
        )?
    );
    runtime.shutdown()?;
    Ok(())
}
