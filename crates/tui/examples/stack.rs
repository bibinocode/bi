//! cargo run -p bi-tui --example stack [-- --headless / --main]
use bi_tui::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{
        Capabilities, ClipRect, ComponentResult, EventResponse, LayoutContext, MouseButton, Offset,
        PointerEvent, PointerKind, ScreenMode,
    },
    runtime::Runtime,
    terminal::{InputModes, RunOptions, TerminalSession, draw_runtime, run},
    widgets::{
        BoxContainer, Editor, HStack, MouseRegion, Padding, SettingItem, SettingsList, StackWidth,
        Text, VStack,
    },
};
use ratatui::{Terminal, backend::TestBackend};
use std::{cell::RefCell, rc::Rc};

struct Status(Rc<RefCell<String>>);
impl Component for Status {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        Text::new(self.0.borrow().as_str()).layout(context, children)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let status = Rc::new(RefCell::new("等待操作".to_owned()));
    let root = runtime.mount_root(Box::new(VStack::new().with_gap(1)))?;
    runtime.append_child(
        root,
        Box::new(Text::new(
            "并排布局：Tab 切换焦点；Alt+Enter 换行；Enter 提交；Escape 退出",
        )),
    )?;
    let stack = runtime.append_child(
        root,
        Box::new(
            HStack::new()
                .with_gap(2)
                .with_widths([StackWidth::Fill(2), StackWidth::Fill(1)]),
        ),
    )?;
    let left =
        runtime.append_child(stack, Box::new(BoxContainer::new(Padding::symmetric(0, 1))))?;
    runtime.append_child(left, Box::new(Text::new("编辑区（2 份宽度）")))?;
    let submitted = Rc::clone(&status);
    let mut editor = Editor::new()
        .with_height(6)
        .on_submit(move |text| *submitted.borrow_mut() = format!("提交：{text}"));
    editor.set_text("中文和 emoji 👩‍💻\n调整终端大小，观察左右两栏自动换行。")?;
    let editor = runtime.append_child(left, Box::new(editor))?;
    let right = runtime.append_child(stack, Box::new(VStack::new().with_gap(1)))?;
    runtime.append_child(right, Box::new(Text::new("设置区（1 份宽度）")))?;
    let changed = Rc::clone(&status);
    runtime.append_child(
        right,
        Box::new(
            SettingsList::new(
                vec![
                    SettingItem::new("mode", "模式", "快速").with_values(["快速", "仔细"]),
                    SettingItem::new("theme", "主题", "默认").with_values(["默认", "暗色"]),
                ],
                3,
            )?
            .on_change(move |id, value| *changed.borrow_mut() = format!("设置：{id} = {value}")),
        ),
    )?;
    let clicked = Rc::clone(&status);
    let region = runtime.append_child(
        right,
        Box::new(MouseRegion::new(move |event, _| {
            if event.kind == PointerKind::Click && event.button == Some(MouseButton::Left) {
                *clicked.borrow_mut() =
                    format!("点击文本：局部坐标 ({}, {})", event.column, event.row);
                Ok(EventResponse {
                    handled: true,
                    redraw: true,
                    ..EventResponse::default()
                })
            } else {
                Ok(EventResponse::default())
            }
        })),
    )?;
    runtime.append_child(region, Box::new(Text::new("[ 点击这段文本 ]")))?;
    runtime.append_child(root, Box::new(Status(Rc::clone(&status))))?;
    runtime.set_focus(Some(editor))?;

    if std::env::args().any(|arg| arg == "--headless") {
        let mut terminal = Terminal::new(TestBackend::new(60, 16))?;
        let layout = runtime
            .layout(&LayoutContext {
                width: 60,
                terminal_size: bi_tui::protocol::Size {
                    width: 60,
                    height: 16,
                },
                available_height: Some(16),
                screen_mode: ScreenMode::Alternate,
                capabilities: Capabilities::default(),
                focused: false,
            })?
            .expect("mounted root");
        // 使用这一帧的完整偏移定位区域，不依赖示例文本换行后的行数。
        let nodes = bi_tui::layout::flatten_layout(
            &layout,
            Offset::default(),
            ClipRect {
                column: 0,
                row: 0,
                width: 60,
                height: 16,
            },
        )?;
        let target = nodes
            .iter()
            .find(|node| node.handle == region)
            .expect("visible region");
        runtime.dispatch_pointer(
            PointerEvent {
                column: target.offset.column,
                row: target.offset.row,
                kind: PointerKind::Click,
                button: Some(MouseButton::Left),
                modifiers: Default::default(),
                click_count: 1,
            },
            &layout,
            Offset::default(),
            ClipRect {
                column: 0,
                row: 0,
                width: 60,
                height: 16,
            },
        )?;
        assert!(status.borrow().starts_with("点击文本："));
        draw_runtime(
            &mut runtime,
            &mut terminal,
            ScreenMode::Alternate,
            Capabilities::default(),
        )?;
        for row in 0..16 {
            let mut line = String::new();
            let mut column = 0;
            while column < 60 {
                let symbol = terminal.backend().buffer()[(column, row)].symbol();
                line.push_str(symbol);
                column += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            println!("{row:02} |{}|", line.trim_end());
        }
        runtime.shutdown()?;
    } else {
        let mut session = if std::env::args().any(|arg| arg == "--main") {
            TerminalSession::main_screen_with_input(16, InputModes::interactive())?
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
