//! cargo run -p bi-tui --example interactive
//! 添加 -- --main 可改用主屏幕的固定高度内联视口。
use bi_tui::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{ComponentResult, LayoutContext, Style},
    runtime::Runtime,
    terminal::{InputModes, RunOptions, TerminalSession, run},
    widgets::{
        BoxContainer, Container, Input, Loader, Padding, ScrollView, SelectItem, SelectList,
        Spacer, Text, TruncatedText,
    },
};
use std::{cell::RefCell, rc::Rc};

// 状态文本属于示例业务；输入回调修改共享值，事件响应请求下一帧重绘。
struct Status(Rc<RefCell<String>>);
impl Component for Status {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        TruncatedText::new(self.0.borrow().clone()).layout(context, children)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let status = Rc::new(RefCell::new(String::from(
        "点击输入框、列表或滚动区域可切换焦点。",
    )));
    let mut runtime = Runtime::new();
    let root = runtime.mount_root(Box::new(Container::new()))?;
    runtime.append_child(
        root,
        Box::new(
            TruncatedText::new("Rust TUI：输入 / 列表 / 滚动 / 动画").with_style(Style {
                bold: true,
                ..Style::default()
            }),
        ),
    )?;
    runtime.append_child(root, Box::new(Text::new("Tab/Shift+Tab 切换焦点。输入：Enter 提交，Ctrl+Z 撤销。列表：↑↓选择、Enter确认。滚动：滚轮、PageUp/PageDown。Escape 或 Ctrl+C 退出。")))?;
    runtime.append_child(root, Box::new(Spacer::new(1)))?;

    let panel =
        runtime.append_child(root, Box::new(BoxContainer::new(Padding::symmetric(1, 2))))?;
    let input_status = Rc::clone(&status);
    let input = runtime.append_child(
        panel,
        Box::new(
            Input::new()
                .with_placeholder("输入中文或粘贴文本")
                .on_submit(move |value| {
                    *input_status.borrow_mut() = format!("输入已提交：{value}");
                }),
        ),
    )?;
    let select_status = Rc::clone(&status);
    runtime.append_child(
        panel,
        Box::new(
            SelectList::new(
                vec![
                    SelectItem::new("rust", "Rust").with_description("系统编程"),
                    SelectItem::new("typescript", "TypeScript").with_description("参考实现"),
                    SelectItem::new("python", "Python").with_description("脚本与工具"),
                ],
                3,
            )
            .on_select(move |item| {
                *select_status.borrow_mut() = format!("已选择：{} ({})", item.label, item.value);
            }),
        ),
    )?;

    let scroll = runtime.append_child(root, Box::new(ScrollView::new(5)))?;
    let transcript = (1..=30)
        .map(|row| format!("记录 {row:02}：这是可滚动的完整文档。"))
        .collect::<Vec<_>>()
        .join("\n");
    runtime.append_child(scroll, Box::new(Text::new(transcript)))?;
    runtime.append_child(
        root,
        Box::new(Loader::new("动画通过消息队列更新；退出时清理定时线程")),
    )?;
    runtime.append_child(root, Box::new(Status(status)))?;
    runtime.set_focus(Some(input))?;

    let mut session = if std::env::args().any(|arg| arg == "--main") {
        TerminalSession::main_screen_with_input(22, InputModes::interactive())?
    } else {
        TerminalSession::alternate_with_input(InputModes::interactive())?
    };
    let result = run(&mut runtime, &mut session, RunOptions::default());
    let restore = session.close();
    result?;
    restore?;
    Ok(())
}
