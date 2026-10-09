mod support;
use bi_tui::{
    component::{Component, LayoutSnapshot},
    protocol::{ComponentEvent, Key, MouseButton, PointerEvent, PointerKind},
    utils::text::{display_width, truncate_text, wrap_text},
    widgets::{Input, ScrollView, SelectItem, SelectList, Text, TruncatedText},
};
use std::{cell::RefCell, rc::Rc};
use support::{Host, context, key, node};

fn text(snapshot: &LayoutSnapshot) -> Vec<String> {
    snapshot
        .lines
        .iter()
        .map(|line| line.spans.iter().map(|span| span.text.as_str()).collect())
        .collect()
}

#[test]
fn unicode_wrap_preserves_graphemes_and_empty_lines() {
    assert_eq!(
        wrap_text("中文e\u{301}\r\n\n", 4).unwrap(),
        ["中文", "e\u{301}", "", ""]
    );
    assert_eq!(wrap_text("👩‍💻", 1).unwrap(), ["�"]);
    assert_eq!(wrap_text("hello", 0).unwrap(), Vec::<String>::new());
    assert!(wrap_text("bad\x1b[31m", 20).is_err());
}

#[test]
fn truncation_never_splits_a_grapheme_or_exceeds_width() {
    assert_eq!(truncate_text("中文测试", 5, "…").unwrap(), "中文…");
    assert_eq!(truncate_text("中文", 1, "...").unwrap(), ".");
    assert_eq!(truncate_text("e\u{301}abc", 2, "…").unwrap(), "e\u{301}…");
    let mut widget = TruncatedText::new("第一行很长\n第二行");
    let snapshot = widget.layout(&context(5, None), &mut []).unwrap();
    assert_eq!(text(&snapshot), ["第一…"]);
    assert!(
        widget
            .layout(&context(5, Some(0)), &mut [])
            .unwrap()
            .lines
            .is_empty()
    );
}

#[test]
fn input_edits_combining_characters_and_emoji_as_units() {
    let mut input = Input::new();
    let mut host = Host::default();
    input.set_value("Ae\u{301}👩‍💻中").unwrap();
    input
        .handle_event(&ComponentEvent::Key(key(Key::Backspace)), &mut host)
        .unwrap();
    assert_eq!(input.value(), "Ae\u{301}👩‍💻");
    input
        .handle_event(&ComponentEvent::Key(key(Key::Left)), &mut host)
        .unwrap();
    input
        .handle_event(&ComponentEvent::Key(key(Key::Backspace)), &mut host)
        .unwrap();
    assert_eq!(input.value(), "A👩‍💻");
    input
        .handle_event(&ComponentEvent::Key(key(Key::Delete)), &mut host)
        .unwrap();
    assert_eq!(input.value(), "A");
}

#[test]
fn input_normalizes_paste_rejects_controls_and_supports_undo() {
    let mut input = Input::new();
    let mut host = Host::default();
    input
        .handle_event(&ComponentEvent::Paste("a\r\nb\tc".into()), &mut host)
        .unwrap();
    assert_eq!(input.value(), "ab    c");
    assert!(
        input
            .handle_event(&ComponentEvent::Text("\x1b".into()), &mut host)
            .is_err()
    );
    assert_eq!(input.value(), "ab    c");
    let mut undo = key(Key::Character('z'));
    undo.modifiers.control = true;
    input
        .handle_event(&ComponentEvent::Key(undo), &mut host)
        .unwrap();
    assert_eq!(input.value(), "");
}

#[test]
fn input_scrolls_horizontally_and_keeps_cursor_inside_narrow_views() {
    let mut input = Input::new().with_prompt("");
    input.set_value("中文很长的输入内容").unwrap();
    for width in 1..=8 {
        let snapshot = input.layout(&context(width, None), &mut []).unwrap();
        assert!(snapshot.cursor.unwrap().position.column < width);
        assert!(display_width(&text(&snapshot)[0]).unwrap() <= usize::from(width));
    }
    assert!(
        input
            .layout(&context(0, None), &mut [])
            .unwrap()
            .cursor
            .is_none()
    );
}

#[test]
fn input_state_roundtrip_and_invalid_restore_are_atomic() {
    let mut input = Input::new();
    input.set_value("中文abc").unwrap();
    input
        .handle_event(&ComponentEvent::Key(key(Key::Left)), &mut Host::default())
        .unwrap();
    let state = input.save_state().unwrap().unwrap();
    let mut restored = Input::new();
    restored.restore_state(&state).unwrap();
    assert_eq!(
        (restored.value(), restored.cursor()),
        (input.value(), input.cursor())
    );
    let mut invalid = state;
    invalid.payload[..8].copy_from_slice(&1u64.to_le_bytes());
    assert!(restored.restore_state(&invalid).is_err());
    assert_eq!(restored.value(), input.value());
}

#[test]
fn input_inserted_combining_mark_keeps_cursor_on_grapheme_boundary() {
    let mut input = Input::new();
    let mut host = Host::default();
    input.set_value("ab").unwrap();
    input
        .handle_event(&ComponentEvent::Key(key(Key::Home)), &mut host)
        .unwrap();
    input
        .handle_event(&ComponentEvent::Key(key(Key::Right)), &mut host)
        .unwrap();
    input
        .handle_event(&ComponentEvent::Text("\u{301}".into()), &mut host)
        .unwrap();
    input
        .handle_event(&ComponentEvent::Key(key(Key::Backspace)), &mut host)
        .unwrap();
    assert_eq!(input.value(), "b");
}

#[test]
fn scroll_view_measures_full_document_and_clamps_after_shrink() {
    let mut scroll = ScrollView::new(2);
    let mut children = [node(2, Text::new("a\nb\nc\nd"))];
    let first = scroll.layout(&context(10, Some(1)), &mut children).unwrap();
    assert_eq!(
        (first.height, first.children[0].node.snapshot.height),
        (1, 4)
    );
    scroll.scroll_to(99);
    let next = scroll.layout(&context(10, Some(1)), &mut children).unwrap();
    assert_eq!(next.children[0].offset.row, -3);
    children[0] = node(2, Text::new("a"));
    scroll.layout(&context(10, None), &mut children).unwrap();
    assert_eq!(scroll.scroll_top(), 0);
    assert!(scroll.layout(&context(10, None), &mut []).is_err());
}

#[test]
fn scroll_follow_end_pauses_when_user_scrolls_up_and_chains_at_boundary() {
    let mut scroll = ScrollView::new(2).with_follow_end(true);
    let mut children = [node(2, Text::new("a\nb\nc\nd"))];
    scroll.layout(&context(10, None), &mut children).unwrap();
    assert_eq!(scroll.scroll_top(), 2);
    assert!(scroll.scroll_by(-1));
    assert!(!scroll.is_following_end());
    children[0] = node(2, Text::new("a\nb\nc\nd\ne"));
    scroll.layout(&context(10, None), &mut children).unwrap();
    assert_eq!(scroll.scroll_top(), 1);
    scroll.scroll_to(usize::MAX);
    assert!(scroll.is_following_end());
    let event = ComponentEvent::Pointer(PointerEvent {
        column: 0,
        row: 0,
        kind: PointerKind::Scroll {
            columns: 0,
            rows: 1,
        },
        button: None,
        modifiers: Default::default(),
        click_count: 0,
    });
    assert!(
        !scroll
            .handle_event(&event, &mut Host::default())
            .unwrap()
            .handled
    );
}

#[test]
fn selection_filters_cycles_and_submits_correct_item() {
    let selected = Rc::new(RefCell::new(String::new()));
    let output = Rc::clone(&selected);
    let mut list = SelectList::new(
        vec![
            SelectItem::new("rust", "Rust"),
            SelectItem::new("ruby", "Ruby"),
            SelectItem::new("python", "Python"),
        ],
        2,
    )
    .on_select(move |item| *output.borrow_mut() = item.value.clone());
    let mut host = Host::default();
    list.handle_event(&ComponentEvent::Key(key(Key::Up)), &mut host)
        .unwrap();
    assert_eq!(list.selected_item().unwrap().value, "python");
    list.set_filter("RU");
    list.handle_event(&ComponentEvent::Key(key(Key::Down)), &mut host)
        .unwrap();
    list.handle_event(&ComponentEvent::Key(key(Key::Enter)), &mut host)
        .unwrap();
    assert_eq!(&*selected.borrow(), "ruby");
    list.set_filter("missing");
    assert!(list.selected_item().is_none());
    assert_eq!(
        list.layout(&context(10, Some(1)), &mut []).unwrap().height,
        1
    );
}

#[test]
fn selection_mouse_uses_visible_window_offset() {
    let mut list = SelectList::new(
        (0..10)
            .map(|i| SelectItem::new(i.to_string(), i.to_string()))
            .collect(),
        3,
    );
    list.set_selected_index(8);
    list.layout(&context(10, None), &mut []).unwrap();
    let response = list
        .handle_event(
            &ComponentEvent::Pointer(PointerEvent {
                column: 0,
                row: 0,
                kind: PointerKind::Press,
                button: Some(MouseButton::Left),
                modifiers: Default::default(),
                click_count: 0,
            }),
            &mut Host::default(),
        )
        .unwrap();
    assert!(response.request_focus);
    assert_eq!(list.selected_item().unwrap().value, "7");
}
