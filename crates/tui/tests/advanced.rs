mod support;
use bi_tui::{
    autocomplete::{AutocompleteProvider, CommandAutocomplete, PathAutocomplete, SlashCommand},
    component::{Component, LayoutSnapshot},
    protocol::{
        Capabilities, ClipRect, ComponentEvent, ImageProtocol, Key, Line, Offset, PointerEvent,
        PointerKind, Size, Span, Style,
    },
    render::{paint_layout, paint_layout_with_images},
    runtime::Runtime,
    terminal::TerminalOutput,
    utils::{fuzzy::fuzzy_score, styled_text::wrap_lines},
    widgets::{
        Editor, Flash, HStack, Image, Input, Markdown, Overlay, OverlayPlacement, ScrollView,
        SearchView, SelectItem, SettingItem, SettingsList, StackAlign, StackEntry, VStack,
    },
};
use ratatui::{buffer::Buffer, layout::Rect};
use std::{cell::RefCell, io::Cursor, rc::Rc, time::Duration};
use support::{Host, context, key, node};

fn text(snapshot: &LayoutSnapshot) -> String {
    snapshot
        .lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
fn ctrl(ch: char) -> ComponentEvent {
    let mut e = key(Key::Character(ch));
    e.modifiers.control = true;
    ComponentEvent::Key(e)
}
fn alt(ch: char) -> ComponentEvent {
    let mut e = key(Key::Character(ch));
    e.modifiers.alt = true;
    ComponentEvent::Key(e)
}

#[test]
fn markdown_parses_styles_tables_tasks_links_and_highlights_code_at_narrow_widths() {
    let mut markdown = Markdown::new(
        "# 标题\n\n**粗体** *斜体* ~~删除~~ [链接](https://example.org)\n\n- [x] 完成\n\n| A | B |\n|---|---|\n| 中 | 文 |\n\n```rust\nfn main() { println!(\"hello\"); }\n```\n\n<fake>\x1b[31m",
    );
    let snapshot = markdown.layout(&context(40, None), &mut []).unwrap();
    assert!(text(&snapshot).contains("标题"));
    assert!(text(&snapshot).contains("[x]"));
    assert!(text(&snapshot).contains("fn main()"));
    assert!(!text(&snapshot).contains('\x1b'));
    assert!(
        snapshot
            .lines
            .iter()
            .flat_map(|l| &l.spans)
            .any(|s| s.style.bold)
    );
    assert!(
        snapshot
            .lines
            .iter()
            .flat_map(|l| &l.spans)
            .any(|s| s.hyperlink.as_deref() == Some("https://example.org"))
    );
    assert!(
        snapshot
            .lines
            .iter()
            .flat_map(|l| &l.spans)
            .any(|s| matches!(s.style.foreground, Some(bi_tui::protocol::Color::Rgb(..))))
    );
    assert_eq!(
        text(
            &Markdown::new("> 引用\n\n- 列表")
                .layout(&context(20, None), &mut [])
                .unwrap()
        ),
        "│ 引用\n• 列表"
    );
    for width in 0..10 {
        let mut root = node(1, Markdown::new(markdown.text()));
        bi_tui::layout::validate_layout(&root.layout(&context(width, Some(5))).unwrap()).unwrap();
    }
}

#[test]
fn styled_wrapping_preserves_split_graphemes_and_hyperlinks() {
    let line = Line {
        spans: vec![
            Span {
                text: "e".into(),
                style: Style {
                    bold: true,
                    ..Style::default()
                },
                hyperlink: Some("https://example.org".into()),
            },
            Span::plain("\u{301}中文"),
        ],
    };
    let lines = wrap_lines(&[line], 3).unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].spans[0].text, "e\u{301}");
    assert!(lines[0].spans[0].style.bold);
}

#[test]
fn fuzzy_commands_match_subsequences_and_rank_contiguous_matches() {
    assert!(fuzzy_score("中文", "这是中文").is_some());
    assert!(fuzzy_score("xyz", "abc").is_none());
    assert!(fuzzy_score("ab", "ab").unwrap() > fuzzy_score("ab", "a long b").unwrap());
    let provider = CommandAutocomplete::new(vec![SlashCommand::new("history")])
        .unwrap()
        .with_fuzzy(true);
    assert_eq!(
        provider.suggestions("/hsy arg", 4).unwrap()[0].replacement,
        "/history"
    );
}

#[test]
fn latex_formats_unicode_math_and_preserves_unsupported_markdown_source() {
    assert!(bi_tui::utils::latex::render_latex(&format!("{}x", "\\sqrt".repeat(1000))).is_none());
    use bi_tui::utils::latex::render_latex;
    assert_eq!(render_latex("x^2+\\alpha_1"), Some("x²+α₁".into()));
    assert_eq!(
        render_latex("\\sqrt{\\frac{a}{b}}"),
        Some("√((a)/(b))".into())
    );
    assert_eq!(render_latex("\\mathbb{R}"), Some("ℝ".into()));
    assert!(render_latex("\\unknown{a}").is_none());
    assert!(render_latex("{a").is_none());
    let mut markdown = Markdown::new("$x^2$ $\\unknown{a}$");
    let rendered = text(&markdown.layout(&context(40, None), &mut []).unwrap());
    assert!(rendered.contains("x²"));
    assert!(rendered.contains("\\unknown{a}"));
}

#[test]
fn path_completion_handles_unicode_spaces_directory_suffix_and_arguments() {
    let directory = std::env::temp_dir().join(format!("bi-tui-completion-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("中文 file.rs"), b"hello").unwrap();
    std::fs::create_dir_all(directory.join("folder")).unwrap();
    let provider = PathAutocomplete::new(&directory);
    let source = "read 中 trailing";
    let candidates = provider.suggestions(source, "read 中".len()).unwrap();
    assert_eq!(candidates[0].replacement, "\"中文 file.rs\"");
    assert_eq!(&source[candidates[0].range.clone()], "中");
    assert_eq!(
        provider.suggestions("fol", 3).unwrap()[0].replacement,
        "folder/"
    );
    let quoted = "read \"中文 f\" trailing";
    let result = provider.suggestions(quoted, "read \"中文 f".len()).unwrap();
    assert_eq!(&quoted[result[0].range.clone()], "\"中文 f\"");
    assert!(provider.suggestions("a", 99).is_err());
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn automatic_completion_shows_single_candidate_without_replacing_text() {
    let mut editor = Editor::new()
        .with_autocomplete(CommandAutocomplete::new(vec![SlashCommand::new("help")]).unwrap())
        .with_auto_complete(true);
    let mut host = Host::default();
    editor
        .handle_event(&ComponentEvent::Text("/h".into()), &mut host)
        .unwrap();
    assert_eq!(editor.text(), "/h");
    assert!(text(&editor.layout(&context(30, None), &mut []).unwrap()).contains("/help"));
    editor
        .handle_event(&ComponentEvent::Key(key(Key::Enter)), &mut host)
        .unwrap();
    assert_eq!(editor.text(), "/help");
}

#[test]
fn input_kills_accumulate_yank_pop_and_typing_coalesces_undo() {
    let mut input = Input::new();
    let mut host = Host::default();
    input
        .handle_event(&ComponentEvent::Text("one ".into()), &mut host)
        .unwrap();
    input
        .handle_event(&ComponentEvent::Text("two".into()), &mut host)
        .unwrap();
    input.handle_event(&ctrl('z'), &mut host).unwrap();
    assert_eq!(input.value(), "");
    input.set_value("one two").unwrap();
    input.handle_event(&ctrl('w'), &mut host).unwrap();
    input.handle_event(&ctrl('w'), &mut host).unwrap();
    input.handle_event(&ctrl('y'), &mut host).unwrap();
    assert_eq!(input.value(), "one two");
    input.set_value("third").unwrap();
    input.handle_event(&ctrl('u'), &mut host).unwrap();
    input.handle_event(&ctrl('y'), &mut host).unwrap();
    input.handle_event(&alt('y'), &mut host).unwrap();
    assert_eq!(input.value(), "one two");
}

#[test]
fn editor_kill_expands_paste_and_yank_undo_restores_document() {
    let mut editor = Editor::new();
    let mut host = Host::default();
    let pasted = ["中文"; 12].join("\n");
    editor
        .handle_event(&ComponentEvent::Paste(pasted.clone()), &mut host)
        .unwrap();
    editor.handle_event(&ctrl('u'), &mut host).unwrap();
    assert_eq!(editor.text(), "");
    editor.handle_event(&ctrl('y'), &mut host).unwrap();
    assert_eq!(editor.expanded_text(), pasted);
    editor.handle_event(&ctrl('z'), &mut host).unwrap();
    assert_eq!(editor.text(), "");
    editor.handle_event(&ctrl('z'), &mut host).unwrap();
    assert_eq!(editor.expanded_text(), pasted);
    let mut typed = Editor::new();
    typed
        .handle_event(&ComponentEvent::Text("a".into()), &mut host)
        .unwrap();
    typed
        .handle_event(&ComponentEvent::Text("b".into()), &mut host)
        .unwrap();
    typed.handle_event(&ctrl('z'), &mut host).unwrap();
    assert_eq!(typed.text(), "");
}

#[test]
fn flex_constraints_redistribute_capped_growth_and_shrink_to_minimums() {
    let mut children = [
        node(1, bi_tui::widgets::Text::new("left")),
        node(2, bi_tui::widgets::Text::new("right")),
    ];
    let mut stack = HStack::new().with_entries([
        StackEntry {
            max: Some(3),
            ..StackEntry::fill(1)
        },
        StackEntry::fill(1),
    ]);
    let snapshot = stack.layout(&context(10, None), &mut children).unwrap();
    assert_eq!(snapshot.children[0].node.snapshot.width, 3);
    assert_eq!(snapshot.children[1].node.snapshot.width, 7);
    let snapshot = HStack::new()
        .with_entries([
            StackEntry {
                basis: Some(8),
                min: 2,
                ..StackEntry::default()
            },
            StackEntry {
                basis: Some(8),
                min: 2,
                ..StackEntry::default()
            },
        ])
        .layout(&context(6, None), &mut children)
        .unwrap();
    assert_eq!(
        snapshot
            .children
            .iter()
            .map(|c| c.node.snapshot.width)
            .collect::<Vec<_>>(),
        [3, 3]
    );
    let invalid = HStack::new()
        .with_entries([StackEntry {
            min: 5,
            max: Some(2),
            ..StackEntry::default()
        }])
        .layout(&context(10, None), &mut children);
    assert!(invalid.is_err());
    let snapshot = VStack::new()
        .with_align(StackAlign::Stretch)
        .with_widths([1, 1])
        .with_entries([StackEntry::fill(1), StackEntry::fill(2)])
        .layout(&context(10, Some(9)), &mut children)
        .unwrap();
    assert_eq!(snapshot.height, 9);
    assert_eq!(snapshot.children[0].clip.unwrap().height, 3);
    assert_eq!(snapshot.children[1].clip.unwrap().height, 6);
    assert_eq!(snapshot.children[0].node.snapshot.width, 10);
}

#[test]
fn responsive_hidden_nodes_are_not_in_tab_traversal() {
    let mut runtime = Runtime::new();
    let root = runtime
        .mount_root(Box::new(HStack::new().with_entries([
            StackEntry::fill(1),
            StackEntry {
                visible_from_width: 30,
                ..StackEntry::fill(1)
            },
        ])))
        .unwrap();
    let first = runtime.append_child(root, Box::new(Input::new())).unwrap();
    runtime.append_child(root, Box::new(Input::new())).unwrap();
    runtime.layout(&context(10, None)).unwrap();
    runtime.focus_next(false).unwrap();
    runtime.focus_next(false).unwrap();
    assert_eq!(runtime.focused_handle(), Some(first));
    runtime.shutdown().unwrap();
}

#[test]
fn overlay_clears_underlying_text_and_modal_scope_restores_focus() {
    let mut runtime = Runtime::new();
    let root = runtime
        .mount_root(Box::new(Overlay::new([OverlayPlacement::centered(6, 2)])))
        .unwrap();
    let base = runtime.append_child(root, Box::new(Input::new())).unwrap();
    let popup = runtime.append_child(root, Box::new(Input::new())).unwrap();
    runtime.set_focus(Some(base)).unwrap();
    runtime.layout(&context(20, Some(6))).unwrap();
    runtime.set_focus_scope(Some(popup)).unwrap();
    assert_eq!(runtime.focused_handle(), Some(popup));
    assert!(runtime.set_focus(Some(base)).is_err());
    runtime.focus_next(false).unwrap();
    assert_eq!(runtime.focused_handle(), Some(popup));
    runtime.remove(popup).unwrap();
    assert_eq!(runtime.focused_handle(), Some(base));
    runtime.shutdown().unwrap();
    let mut root = node(1, Overlay::new([OverlayPlacement::centered(4, 2).at(2, 0)]));
    root.add_child(node(2, bi_tui::widgets::Text::new("abcdefgh\nabcdefgh")));
    root.add_child(node(3, bi_tui::widgets::Text::new("X")));
    let layout = root.layout(&context(8, Some(2))).unwrap();
    let mut buffer = Buffer::empty(Rect::new(0, 0, 8, 2));
    paint_layout(&layout, Offset::default(), &mut buffer).unwrap();
    assert_eq!(buffer[(2, 0)].symbol(), "X");
    assert_eq!(buffer[(3, 0)].symbol(), " ");
    assert_eq!(buffer[(3, 1)].symbol(), " ");
}

#[test]
fn scroll_remainder_and_scrollbar_reserve_their_own_column() {
    let mut view = ScrollView::new(3).with_scrollbar(true);
    let mut children = [node(1, bi_tui::widgets::Text::new("a\nb\nc\nd\ne"))];
    let snapshot = view.layout(&context(10, None), &mut children).unwrap();
    assert_eq!(snapshot.children[0].node.snapshot.width, 9);
    assert!(text(&snapshot).contains('█'));
    let mut host = Host::default();
    let response = view
        .handle_event(
            &ComponentEvent::Pointer(PointerEvent {
                column: 0,
                row: 0,
                kind: PointerKind::Scroll {
                    columns: 0,
                    rows: 5,
                },
                button: None,
                modifiers: Default::default(),
                click_count: 0,
            }),
            &mut host,
        )
        .unwrap();
    assert_eq!(response.scroll_remainder, Some(3));
    assert!(!response.handled);
    assert_eq!(view.scroll_top(), 2);
    let pointer = |kind, row| {
        ComponentEvent::Pointer(PointerEvent {
            column: 9,
            row,
            kind,
            button: Some(bi_tui::protocol::MouseButton::Left),
            modifiers: Default::default(),
            click_count: 0,
        })
    };
    let response = view
        .handle_event(&pointer(PointerKind::Press, 0), &mut host)
        .unwrap();
    assert_eq!(
        response.pointer_capture,
        bi_tui::protocol::PointerCapture::Acquire
    );
    view.handle_event(&pointer(PointerKind::Drag, 100), &mut host)
        .unwrap();
    assert_eq!(view.scroll_top(), 2);
    let response = view
        .handle_event(&pointer(PointerKind::Release, 100), &mut host)
        .unwrap();
    assert_eq!(
        response.pointer_capture,
        bi_tui::protocol::PointerCapture::Release
    );
}

#[test]
fn search_jumps_to_document_lines_and_escape_clears_highlighting() {
    let mut view = SearchView::new(3);
    let mut children = [node(
        1,
        bi_tui::widgets::Text::new("zero\n中文\ntwo\n中文\nend"),
    )];
    let mut host = Host::default();
    view.handle_event(&ctrl('f'), &mut host).unwrap();
    view.handle_event(&ComponentEvent::Text("中文".into()), &mut host)
        .unwrap();
    let snapshot = view.layout(&context(20, None), &mut children).unwrap();
    assert_eq!(view.match_count(), 2);
    assert_eq!(snapshot.children[0].offset.row, 0);
    view.handle_event(&ComponentEvent::Key(key(Key::Enter)), &mut host)
        .unwrap();
    let snapshot = view.layout(&context(20, None), &mut children).unwrap();
    assert_eq!(snapshot.children[0].offset.row, -2);
    view.handle_event(&ComponentEvent::Key(key(Key::Escape)), &mut host)
        .unwrap();
    view.layout(&context(20, None), &mut children).unwrap();
    assert_eq!(view.match_count(), 0);
}

#[test]
fn dynamic_settings_submenu_commits_selection_and_escape_only_closes_menu() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&changes);
    let mut settings = SettingsList::new(vec![SettingItem::new("mode", "模式", "old")], 5)
        .unwrap()
        .with_submenu(|_| {
            Ok(vec![
                SelectItem::new("one", "一"),
                SelectItem::new("two", "二"),
            ])
        })
        .on_change(move |_, v| log.borrow_mut().push(v.to_owned()));
    let mut host = Host::default();
    settings
        .handle_event(&ComponentEvent::Key(key(Key::Enter)), &mut host)
        .unwrap();
    settings
        .handle_event(&ComponentEvent::Key(key(Key::Down)), &mut host)
        .unwrap();
    settings
        .handle_event(&ComponentEvent::Key(key(Key::Enter)), &mut host)
        .unwrap();
    assert_eq!(settings.items()[0].current_value, "two");
    assert_eq!(changes.borrow().as_slice(), ["two"]);
    settings
        .handle_event(&ComponentEvent::Key(key(Key::Enter)), &mut host)
        .unwrap();
    assert!(
        settings
            .handle_event(&ComponentEvent::Key(key(Key::Escape)), &mut host)
            .unwrap()
            .handled
    );
}

#[test]
fn flash_expiry_is_message_driven_and_unmount_cancels_timer() {
    let mut flash = Flash::new(Duration::ZERO);
    let mut host = Host::default();
    flash
        .handle_event(
            &ComponentEvent::Message {
                topic: "bi.flash".into(),
                payload: "完成".as_bytes().to_vec(),
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(
        text(&flash.layout(&context(10, None), &mut []).unwrap()),
        "完成"
    );
    flash
        .handle_event(
            &ComponentEvent::Message {
                topic: "bi.flash.tick".into(),
                payload: Vec::new(),
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(flash.layout(&context(10, None), &mut []).unwrap().height, 0);
    let mut runtime = Runtime::new();
    runtime.mount_root(Box::new(Flash::default())).unwrap();
    runtime.shutdown().unwrap();
}

fn png() -> Vec<u8> {
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        16,
        8,
        image::Rgba([255, 0, 0, 255]),
    ));
    let mut data = Cursor::new(Vec::new());
    image.write_to(&mut data, image::ImageFormat::Png).unwrap();
    data.into_inner()
}

#[test]
fn image_protocols_encode_cropped_png_and_remove_only_owned_kitty_ids() {
    for protocol in [ImageProtocol::Kitty, ImageProtocol::Iterm2] {
        let mut root = node(
            1,
            Image::new(
                "test",
                png(),
                Size {
                    width: 4,
                    height: 2,
                },
            )
            .unwrap(),
        );
        let mut context = context(4, Some(2));
        context.capabilities.images = Some(protocol);
        let layout = root.layout(&context).unwrap();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 2));
        paint_layout_with_images(&layout, Offset::default(), &mut buffer).unwrap();
        let viewport = ClipRect {
            column: 0,
            row: 1,
            width: 4,
            height: 1,
        };
        let mut output = TerminalOutput::default();
        let mut bytes = Vec::new();
        output
            .write_frame(
                &mut bytes,
                &layout,
                Offset::default(),
                viewport,
                &buffer,
                context.capabilities,
            )
            .unwrap();
        let encoded = String::from_utf8(bytes).unwrap();
        assert!(encoded.contains("\x1b[2;1H"));
        if protocol == ImageProtocol::Kitty {
            assert!(encoded.contains("f=100"));
            assert!(encoded.contains("c=4,r=1"));
            let mut cleanup = Vec::new();
            output.clear(&mut cleanup).unwrap();
            assert!(String::from_utf8(cleanup).unwrap().contains("a=d,d=I,i="));
        } else {
            assert!(encoded.contains("1337;File=inline=1;width=4;height=1"));
            assert!(output.needs_clear());
        }
    }
    let mut image = Image::new(
        "test",
        png(),
        Size {
            width: 4,
            height: 2,
        },
    )
    .unwrap()
    .with_alt("图片不可用");
    assert!(text(&image.layout(&context(20, None), &mut []).unwrap()).contains("图片不可用"));
}

#[test]
fn osc8_closes_links_and_removal_reprints_unchanged_cells_without_stale_targets() {
    struct Link(bool);
    impl Component for Link {
        fn layout(
            &mut self,
            c: &bi_tui::protocol::LayoutContext,
            _: &mut [bi_tui::component::ComponentNode],
        ) -> bi_tui::protocol::ComponentResult<LayoutSnapshot> {
            Ok(LayoutSnapshot::from_lines(
                c.width,
                vec![Line {
                    spans: vec![Span {
                        text: "hello".into(),
                        style: Style::default(),
                        hyperlink: self.0.then(|| "https://example.org".into()),
                    }],
                }],
            ))
        }
    }
    let mut output = TerminalOutput::default();
    let mut root = node(1, Link(true));
    let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 1));
    let viewport = ClipRect {
        column: 0,
        row: 0,
        width: 10,
        height: 1,
    };
    let capabilities = Capabilities {
        hyperlinks: true,
        ..Capabilities::default()
    };
    let layout = root.layout(&context(10, None)).unwrap();
    paint_layout(&layout, Offset::default(), &mut buffer).unwrap();
    let mut bytes = Vec::new();
    output
        .write_frame(
            &mut bytes,
            &layout,
            Offset::default(),
            viewport,
            &buffer,
            capabilities,
        )
        .unwrap();
    assert!(
        String::from_utf8(bytes)
            .unwrap()
            .contains("\x1b]8;;https://example.org\x1b\\h\x1b]8;;\x1b\\")
    );
    let mut root = node(1, Link(false));
    let layout = root.layout(&context(10, None)).unwrap();
    let mut bytes = Vec::new();
    output
        .write_frame(
            &mut bytes,
            &layout,
            Offset::default(),
            viewport,
            &buffer,
            capabilities,
        )
        .unwrap();
    let bytes = String::from_utf8(bytes).unwrap();
    assert!(!bytes.contains("https://"));
    assert!(bytes.contains("\x1b]8;;\x1b\\h"));
}

#[test]
fn custom_keybindings_remap_shortcuts_without_inserting_original_text() {
    let mut bindings = bi_tui::keybindings::Keybindings::new();
    bindings.bind("alt+x", "ctrl+u").unwrap();
    bindings.disable("delete").unwrap();
    assert!(bi_tui::keys::KeyChord::parse("ctrl+unknown").is_err());
    let mut runtime = Runtime::new();
    let input = runtime.mount_root(Box::new(Input::new())).unwrap();
    runtime.set_focus(Some(input)).unwrap();
    runtime.dispatch_paste("中文".into()).unwrap();
    runtime.set_keybindings(bindings);
    let mut event = key(Key::Character('x'));
    event.modifiers.alt = true;
    runtime.dispatch_key(event, Some("x".into())).unwrap();
    let snapshot = runtime.layout(&context(20, None)).unwrap().unwrap();
    assert_eq!(text(&snapshot.snapshot), "> ");
    assert!(runtime.dispatch_key(key(Key::Delete), None).unwrap());
    runtime.shutdown().unwrap();
}

#[test]
fn nested_scroll_consumes_only_remaining_rows_at_each_ancestor() {
    let mut runtime = Runtime::new();
    let outer = runtime.mount_root(Box::new(ScrollView::new(2))).unwrap();
    let inner = runtime
        .append_child(outer, Box::new(ScrollView::new(3)))
        .unwrap();
    runtime
        .append_child(inner, Box::new(bi_tui::widgets::Text::new("a\nb\nc\nd\ne")))
        .unwrap();
    let viewport = ClipRect {
        column: 0,
        row: 0,
        width: 10,
        height: 2,
    };
    let layout = runtime.layout(&context(10, Some(2))).unwrap().unwrap();
    runtime
        .dispatch_pointer(
            PointerEvent {
                column: 0,
                row: 0,
                kind: PointerKind::Scroll {
                    rows: 3,
                    columns: 0,
                },
                button: None,
                modifiers: Default::default(),
                click_count: 0,
            },
            &layout,
            Offset::default(),
            viewport,
        )
        .unwrap();
    let layout = runtime.layout(&context(10, Some(2))).unwrap().unwrap();
    assert_eq!(layout.snapshot.children[0].offset.row, -1);
    assert_eq!(
        layout.snapshot.children[0].node.snapshot.children[0]
            .offset
            .row,
        -2
    );
    runtime.shutdown().unwrap();
}

#[test]
fn fully_occluded_image_is_not_encoded_and_invalid_image_output_is_transactional() {
    let mut root = node(1, Overlay::new([OverlayPlacement::centered(4, 2)]));
    root.add_child(node(
        2,
        Image::new(
            "test",
            png(),
            Size {
                width: 4,
                height: 2,
            },
        )
        .unwrap(),
    ));
    root.add_child(node(3, bi_tui::widgets::Text::new("cover")));
    let mut c = context(4, Some(2));
    c.capabilities.images = Some(ImageProtocol::Kitty);
    let layout = root.layout(&c).unwrap();
    let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 2));
    paint_layout_with_images(&layout, Offset::default(), &mut buffer).unwrap();
    let mut output = TerminalOutput::default();
    let mut bytes = Vec::new();
    output
        .write_frame(
            &mut bytes,
            &layout,
            Offset::default(),
            ClipRect {
                column: 0,
                row: 0,
                width: 4,
                height: 2,
            },
            &buffer,
            c.capabilities,
        )
        .unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains("f=100"));
    let mut image = node(
        1,
        Image::new(
            "test",
            png(),
            Size {
                width: 4,
                height: 2,
            },
        )
        .unwrap(),
    );
    let mut layout = image.layout(&c).unwrap();
    layout.snapshot.images[0].data = std::sync::Arc::from(b"invalid".as_slice());
    let mut bytes = Vec::new();
    assert!(
        output
            .write_frame(
                &mut bytes,
                &layout,
                Offset::default(),
                ClipRect {
                    column: 0,
                    row: 0,
                    width: 4,
                    height: 2
                },
                &buffer,
                c.capabilities
            )
            .is_err()
    );
    assert!(bytes.is_empty());
}
