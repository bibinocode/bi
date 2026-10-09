use bi_tui::terminal::{TerminalEvent, TerminalSession, read_event};

use bi_tui::component::ComponentNode;
use bi_tui::protocol::{Capabilities, ComponentHandle, ComponentId, LayoutContext, Offset, Size};
use bi_tui::render::paint_layout;
use bi_tui::widgets::{BoxContainer, Padding, Spacer, Text};

type ExampleResult<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> ExampleResult<()> {
    let mut session = TerminalSession::main_screen(8)?;

    let result = run(&mut session);
    let close_result = session.close();

    // 无论 run 是否成功，上面都已经执行显式恢复。
    result?;
    close_result?;

    Ok(())
}

fn run(session: &mut TerminalSession) -> ExampleResult<()> {
    let mut root = ComponentNode::new(
        ComponentHandle::initial(ComponentId(1)),
        Box::new(BoxContainer::new(Padding::symmetric(1, 2))),
    );

    root.add_child(ComponentNode::new(
        ComponentHandle::initial(ComponentId(2)),
        Box::new(Text::new("你好，Rust TUI")),
    ));

    root.add_child(ComponentNode::new(
        ComponentHandle::initial(ComponentId(3)),
        Box::new(Spacer::new(1)),
    ));

    root.add_child(ComponentNode::new(
        ComponentHandle::initial(ComponentId(4)),
        Box::new(Text::new("调整窗口大小可以观察自动换行。\n按任意键退出。")),
    ));

    let screen_mode = session.screen_mode();

    loop {
        let terminal_size = session.terminal_mut().size()?;
        let mut render_result = Ok(());

        session.terminal_mut().draw(|frame| {
            let area = frame.area();

            let context = LayoutContext {
                width: area.width,
                terminal_size: Size {
                    width: terminal_size.width,
                    height: terminal_size.height,
                },
                available_height: Some(area.height),
                screen_mode,
                capabilities: Capabilities::default(),
                focused: false,
            };

            render_result = root.layout(&context).and_then(|layout| {
                paint_layout(
                    &layout,
                    Offset {
                        column: i32::from(area.x),
                        row: i64::from(area.y),
                    },
                    frame.buffer_mut(),
                )
            });
        })?;

        render_result?;

        match read_event()? {
            TerminalEvent::Key { .. } => break,

            TerminalEvent::Resize(_) => {
                root.invalidate_subtree();
            }

            _ => {}
        }
    }

    Ok(())
}
