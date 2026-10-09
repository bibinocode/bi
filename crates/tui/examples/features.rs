//! cargo run -p bi-tui --example features [-- --headless / --main / --overlay]
use bi_tui::{
    autocomplete::{CombinedAutocomplete, CommandAutocomplete, PathAutocomplete, SlashCommand},
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{Capabilities, ComponentResult, ImageProtocol, LayoutContext, ScreenMode, Size},
    runtime::Runtime,
    terminal::{InputModes, RunOptions, TerminalOutput, TerminalSession, draw_runtime, run},
    widgets::{
        Editor, Flash, HStack, Image, Markdown, Overlay, OverlayPlacement, SearchView, SelectItem,
        SettingItem, SettingsList, StackEntry, Text, VStack,
    },
};
use ratatui::{Terminal, backend::TestBackend};
use std::{cell::RefCell, io::Cursor, rc::Rc};

struct Status(Rc<RefCell<String>>);
impl Component for Status {
    fn layout(
        &mut self,
        c: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        Text::new(self.0.borrow().as_str()).layout(c, children)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let headless = has("--headless");
    let overlay = has("--overlay");
    let mut capabilities = if headless {
        Capabilities {
            true_color: true,
            hyperlinks: true,
            images: Some(ImageProtocol::Kitty),
        }
    } else {
        bi_tui::terminal::detect_capabilities()
    };
    if has("--kitty") {
        capabilities.images = Some(ImageProtocol::Kitty);
    }
    if has("--iterm2") {
        capabilities.images = Some(ImageProtocol::Iterm2);
    }
    let mut runtime = Runtime::new();
    let log = Rc::new(RefCell::new("等待提交或设置修改".to_owned()));
    let root = runtime.mount_root(Box::new(Overlay::new(if overlay {
        vec![OverlayPlacement::centered(38, 4)]
    } else {
        Vec::new()
    })))?;
    let body = runtime.append_child(root, Box::new(VStack::new().with_gap(1)))?;
    runtime.append_child(
        body,
        Box::new(Text::new(
            "完整功能示例：Tab 切焦点；搜索区 Ctrl+F；编辑区 Alt+Enter 换行；Escape 退出",
        )),
    )?;
    let row = runtime.append_child(
        body,
        Box::new(HStack::new().with_gap(2).with_entries([
            StackEntry::fill(2),
            StackEntry {
                min: 16,
                visible_from_width: 42,
                ..StackEntry::fill(1)
            },
        ])),
    )?;
    let search = runtime.append_child(row, Box::new(SearchView::new(10)))?;
    runtime.append_child(search,Box::new(Markdown::new("# Rust TUI\n\n**Markdown**、*斜体*、~~删除~~ 与 [超链接](https://www.rust-lang.org)。\n\n- [x] 文本折行\n- [x] 代码高亮\n\n```rust\nfn main() {\n    println!(\"中文 👩‍💻\");\n}\n```\n\n搜索中文。\n\n| 模块 | 状态 |\n|---|---|\n| 布局 | 就绪 |\n| 终端 | 就绪 |")))?;
    let side = runtime.append_child(row, Box::new(VStack::new().with_gap(1)))?;
    let pixels = image::RgbaImage::from_fn(64, 32, |x, y| {
        image::Rgba([(x * 4) as u8, (y * 8) as u8, 128, 255])
    });
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels).write_to(&mut png, image::ImageFormat::Png)?;
    runtime.append_child(
        side,
        Box::new(
            Image::new(
                "gradient",
                png.into_inner(),
                Size {
                    width: 16,
                    height: 4,
                },
            )?
            .with_alt("[终端未启用图片协议]"),
        ),
    )?;
    let changed = Rc::clone(&log);
    runtime.append_child(
        side,
        Box::new(
            SettingsList::new(
                vec![SettingItem::new("mode", "模式", "快速").with_values(["快速", "仔细"])],
                3,
            )?
            .with_submenu(|_| {
                Ok(vec![
                    SelectItem::new("快速", "快速"),
                    SelectItem::new("仔细", "仔细"),
                ])
            })
            .on_change(move |id, value| *changed.borrow_mut() = format!("设置：{id} = {value}")),
        ),
    )?;
    let provider = CombinedAutocomplete::new()
        .with_provider(
            CommandAutocomplete::new(vec![
                SlashCommand::new("help"),
                SlashCommand::new("history"),
            ])?
            .with_fuzzy(true),
        )
        .with_provider(PathAutocomplete::new(std::env::current_dir()?).with_fuzzy(true));
    let submitted = Rc::clone(&log);
    let editor = runtime.append_child(
        body,
        Box::new(
            Editor::new()
                .with_height(5)
                .with_autocomplete(provider)
                .with_auto_complete(true)
                .on_submit(move |text| *submitted.borrow_mut() = format!("提交：{text}")),
        ),
    )?;
    let flash = runtime.append_child(body, Box::new(Flash::default()))?;
    runtime.message_sender(flash)?.post(
        flash,
        "bi.flash",
        "Markdown / 补全 / 搜索 / 图片 / 设置 已接入"
            .as_bytes()
            .to_vec(),
    )?;
    runtime.append_child(body, Box::new(Status(log)))?;
    runtime.set_focus(Some(editor))?;
    if overlay {
        let popup = runtime.append_child(
            root,
            Box::new(Text::new(
                "这是通用浮层\n底层内容被遮挡，焦点限制在浮层内\nEscape 退出示例",
            )),
        )?;
        runtime.layout(&LayoutContext {
            width: 80,
            terminal_size: Size {
                width: 80,
                height: 24,
            },
            available_height: Some(24),
            screen_mode: ScreenMode::Alternate,
            capabilities,
            focused: false,
        })?;
        runtime.set_focus_scope(Some(popup))?;
    }
    if headless {
        runtime.drain_messages(64)?;
        let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
        let frame = draw_runtime(
            &mut runtime,
            &mut terminal,
            ScreenMode::Alternate,
            capabilities,
        )?;
        let mut protocol = Vec::new();
        let mut output = TerminalOutput::default();
        output.write_frame(
            &mut protocol,
            frame.root.as_ref().unwrap(),
            frame.offset,
            frame.viewport,
            &frame.buffer,
            capabilities,
        )?;
        assert!(protocol.windows(5).any(|bytes| bytes == b"f=100"));
        for row in 0..24 {
            let mut line = String::new();
            let mut col = 0;
            while col < 80 {
                let symbol = frame.buffer[(col, row)].symbol();
                line.push_str(symbol);
                col += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            println!("{row:02} |{}|", line.trim_end());
        }
        println!("已编码 {} 字节终端协议（未写入真实终端）", protocol.len());
        output.clear(&mut Vec::new())?;
        runtime.shutdown()?;
    } else {
        let mut session = if has("--main") {
            TerminalSession::main_screen_dynamic_with_input(24, InputModes::interactive())?
        } else {
            TerminalSession::alternate_with_input(InputModes::interactive())?
        };
        session.set_cursor_style(crossterm::cursor::SetCursorStyle::SteadyBar)?;
        if has("--main") {
            session.print_scrollback("TUI 功能示例已启动；这行保留在主屏幕滚动历史中。")?;
        }
        let result = run(
            &mut runtime,
            &mut session,
            RunOptions {
                capabilities,
                ..RunOptions::default()
            },
        );
        let restore = session.close();
        result?;
        restore?;
    }
    Ok(())
}
