//! cargo run -p bi-tui --example editor
//! -- --headless：无终端脚本验证；-- --main：主屏幕内联视口。
use bi_tui::{
    autocomplete::{CommandAutocomplete, SlashCommand},
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{
        Capabilities, ComponentResult, Key, KeyEvent, KeyKind, LayoutContext, Modifiers,
        ScreenMode, Style,
    },
    runtime::Runtime,
    terminal::{InputModes, RunOptions, TerminalSession, draw_runtime, run},
    widgets::{
        BoxContainer, CancellableLoader, Container, Editor, Loader, Padding, SettingItem,
        SettingsList, Text, TruncatedText,
    },
};
use ratatui::{Terminal, backend::TestBackend};
use std::{cell::RefCell, rc::Rc};

struct Status(Rc<RefCell<Vec<String>>>);
impl Component for Status {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        let log = self.0.borrow();
        TruncatedText::new(log.last().map_or("等待操作", String::as_str)).layout(context, children)
    }
}
fn key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::default(),
        kind: KeyKind::Press,
        base_layout_key: None,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let headless = std::env::args().any(|arg| arg == "--headless");
    let logs = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = Runtime::new();
    let root = runtime.mount_root(Box::new(Container::new()))?;
    runtime.append_child(
        root,
        Box::new(
            Text::new("多行编辑 / 命令补全 / 设置 / 取消").with_style(Style {
                bold: true,
                ..Style::default()
            }),
        ),
    )?;
    runtime.append_child(root, Box::new(Text::new("Enter 提交；Alt+Enter 换行；Ctrl+Z 撤销；Alt+↑↓ 历史。输入 /h 后 Tab 补全。Tab 切换焦点；设置 Enter 切值；Loader 第一次 Escape 取消，第二次退出。")))?;
    runtime.append_child(root, Box::new(Text::new("编辑器：")))?;
    let panel =
        runtime.append_child(root, Box::new(BoxContainer::new(Padding::symmetric(1, 2))))?;
    let provider = CommandAutocomplete::new(vec![
        SlashCommand::new("help").with_description("帮助"),
        SlashCommand::new("history").with_description("历史"),
        SlashCommand::new("clear").with_description("清空会话"),
    ])?;
    let submitted = Rc::clone(&logs);
    let editor = runtime.append_child(
        panel,
        Box::new(
            Editor::new()
                .with_height(6)
                .with_autocomplete(provider)
                .on_submit(move |text| {
                    submitted.borrow_mut().push(format!("提交：{text}"));
                }),
        ),
    )?;
    let changes = Rc::clone(&logs);
    let settings = runtime.append_child(
        root,
        Box::new(
            SettingsList::new(
                vec![
                    SettingItem::new("mode", "工作模式", "快速")
                        .with_values(["快速", "仔细"])
                        .with_description("示例配置，仅修改内存中的设置值。"),
                    SettingItem::new("theme", "主题", "默认").with_values(["默认", "暗色"]),
                    SettingItem::new("version", "版本（只读）", "0.1.0"),
                ],
                3,
            )?
            .with_search(true)
            .on_change(move |id, value| changes.borrow_mut().push(format!("设置：{id} = {value}"))),
        ),
    )?;
    let aborted = Rc::clone(&logs);
    let loader = if headless {
        Loader::new("可取消任务（示例）").with_frames(vec![".".into()])
    } else {
        Loader::new("可取消任务（示例）")
    };
    let loader = CancellableLoader::from_loader(loader)
        .on_abort(move || aborted.borrow_mut().push("任务已取消".into()));
    let token = loader.token();
    let cancel = runtime.append_child(root, Box::new(loader))?;
    runtime.append_child(root, Box::new(Status(Rc::clone(&logs))))?;
    runtime.set_focus(Some(editor))?;

    if headless {
        runtime.dispatch_paste("/h".into())?;
        runtime.dispatch_key(key(Key::Tab), None)?;
        runtime.dispatch_key(key(Key::Down), None)?;
        runtime.dispatch_key(key(Key::Enter), None)?;
        let mut newline = key(Key::Enter);
        newline.modifiers.alt = true;
        runtime.dispatch_key(newline, None)?;
        runtime.dispatch_paste("中文多行内容".into())?;
        runtime.dispatch_key(key(Key::Enter), None)?;
        assert_eq!(logs.borrow()[0], "提交：/history\n中文多行内容");
        runtime.set_focus(Some(settings))?;
        runtime.dispatch_key(key(Key::Enter), None)?;
        runtime.set_focus(Some(cancel))?;
        runtime.dispatch_key(key(Key::Escape), None)?;
        assert!(token.is_cancelled());
        let mut terminal = Terminal::new(TestBackend::new(60, 22))?;
        draw_runtime(
            &mut runtime,
            &mut terminal,
            ScreenMode::Alternate,
            Capabilities::default(),
        )?;
        for row in 0..22 {
            let mut line = String::new();
            let mut column = 0;
            while column < 60 {
                let symbol = terminal.backend().buffer()[(column, row)].symbol();
                line.push_str(symbol);
                column += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            println!("{row:02} |{}|", line.trim_end());
        }
        for log in logs.borrow().iter() {
            println!("{log}");
        }
        runtime.shutdown()?;
    } else {
        let mut session = if std::env::args().any(|arg| arg == "--main") {
            TerminalSession::main_screen_with_input(22, InputModes::interactive())?
        } else {
            TerminalSession::alternate_with_input(InputModes::interactive())?
        };
        let result = run(&mut runtime, &mut session, RunOptions::default());
        let restore = session.close();
        result?;
        restore?;
    }
    Ok(())
}
