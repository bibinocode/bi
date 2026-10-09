mod support;
use bi_tui::{
    component::LayoutSnapshot,
    protocol::{Capabilities, Color, Line, Offset, ScreenMode, Span, Style},
    render::paint_layout,
    runtime::Runtime,
    terminal::draw_runtime,
    widgets::{BoxContainer, Container, Input, Padding, ScrollView, Spacer, Text},
};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};
use support::{context, node};

#[test]
fn nested_scroll_clips_full_document_and_keeps_padding() {
    let mut runtime = Runtime::new();
    let root = runtime
        .mount_root(Box::new(BoxContainer::new(Padding::symmetric(1, 2))))
        .unwrap();
    let scroll = runtime
        .append_child(root, Box::new(ScrollView::new(2)))
        .unwrap();
    runtime
        .append_child(scroll, Box::new(Text::new("第一行\n第二行\n第三行")))
        .unwrap();
    runtime.set_focus(Some(scroll)).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    draw_runtime(
        &mut runtime,
        &mut terminal,
        ScreenMode::Alternate,
        Capabilities::default(),
    )
    .unwrap();
    assert_eq!(terminal.backend().buffer()[(2, 1)].symbol(), "第");
    runtime
        .dispatch_key(support::key(bi_tui::protocol::Key::Down), None)
        .unwrap();
    draw_runtime(
        &mut runtime,
        &mut terminal,
        ScreenMode::Alternate,
        Capabilities::default(),
    )
    .unwrap();
    assert_eq!(terminal.backend().buffer()[(4, 1)].symbol(), "二");
    assert_eq!(terminal.backend().buffer()[(4, 2)].symbol(), "三");
    assert_eq!(terminal.backend().buffer()[(0, 1)].symbol(), " ");
    assert_eq!(terminal.backend().buffer()[(2, 3)].symbol(), " ");
}

#[test]
fn frame_replacement_clears_old_text_including_wide_cells() {
    let mut runtime = Runtime::new();
    let handle = runtime.mount_root(Box::new(Text::new("中文abc"))).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(10, 2)).unwrap();
    draw_runtime(
        &mut runtime,
        &mut terminal,
        ScreenMode::Alternate,
        Capabilities::default(),
    )
    .unwrap();
    runtime.replace(handle, Box::new(Spacer::new(1))).unwrap();
    draw_runtime(
        &mut runtime,
        &mut terminal,
        ScreenMode::Alternate,
        Capabilities::default(),
    )
    .unwrap();
    for cell in &terminal.backend().buffer().content {
        assert_eq!(cell.symbol(), " ");
    }
}

#[test]
fn cursor_tracks_focus_and_is_hidden_when_clipped() {
    let mut runtime = Runtime::new();
    let root = runtime.mount_root(Box::new(Container::new())).unwrap();
    runtime
        .append_child(root, Box::new(Spacer::new(1)))
        .unwrap();
    let input = runtime.append_child(root, Box::new(Input::new())).unwrap();
    runtime.set_focus(Some(input)).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(10, 2)).unwrap();
    let drawn = draw_runtime(
        &mut runtime,
        &mut terminal,
        ScreenMode::Alternate,
        Capabilities::default(),
    )
    .unwrap();
    let cursor = bi_tui::render::layout_cursor(
        drawn.root.as_ref().unwrap(),
        Some(input),
        drawn.offset,
        drawn.viewport,
    )
    .unwrap();
    assert_eq!(cursor, Some((2, 1)));
    terminal.backend_mut().resize(10, 1);
    let drawn = draw_runtime(
        &mut runtime,
        &mut terminal,
        ScreenMode::Alternate,
        Capabilities::default(),
    )
    .unwrap();
    assert_eq!(
        bi_tui::render::layout_cursor(
            drawn.root.as_ref().unwrap(),
            Some(input),
            drawn.offset,
            drawn.viewport
        )
        .unwrap(),
        None
    );
}

#[test]
fn buffer_offset_and_span_boundary_grapheme_are_respected() {
    struct Spans;
    impl bi_tui::component::Component for Spans {
        fn layout(
            &mut self,
            ctx: &bi_tui::protocol::LayoutContext,
            _: &mut [bi_tui::component::ComponentNode],
        ) -> bi_tui::protocol::ComponentResult<LayoutSnapshot> {
            Ok(LayoutSnapshot::from_lines(
                ctx.width,
                vec![Line {
                    spans: vec![
                        Span::styled(
                            "e",
                            Style {
                                foreground: Some(Color::Indexed(1)),
                                ..Style::default()
                            },
                        ),
                        Span::plain("\u{301}中"),
                    ],
                }],
            ))
        }
    }
    let layout = node(1, Spans).layout(&context(3, None)).unwrap();
    let mut buffer = Buffer::empty(Rect::new(3, 2, 3, 1));
    paint_layout(&layout, Offset { column: 3, row: 2 }, &mut buffer).unwrap();
    assert_eq!(buffer[(3, 2)].symbol(), "e\u{301}");
    assert_eq!(buffer[(3, 2)].fg, ratatui::style::Color::Indexed(1));
    assert_eq!(buffer[(4, 2)].symbol(), "中");
}

#[test]
fn invalid_layout_fails_before_clearing_buffer() {
    let mut layout = node(1, Text::new("ok")).layout(&context(4, None)).unwrap();
    layout.snapshot.lines = vec![Line::plain("too long")];
    let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 1));
    buffer[(0, 0)].set_symbol("X");
    assert!(paint_layout(&layout, Offset::default(), &mut buffer).is_err());
    assert_eq!(buffer[(0, 0)].symbol(), "X");
}
