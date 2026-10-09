mod support;
use bi_tui::{
    protocol::{ComponentEvent, Key, KeyKind, Size},
    runtime::{PointerTracker, Runtime},
    terminal::{LoopControl, RunOptions, TerminalEvent, dispatch_terminal_event, normalize_event},
    widgets::{Container, Input, SelectItem, SelectList},
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[test]
fn normalizer_keeps_text_separate_from_key_and_suppresses_shortcut_text() {
    let plain = normalize_event(Event::Key(KeyEvent::new(
        KeyCode::Char('中'),
        KeyModifiers::NONE,
    )));
    assert!(
        matches!(plain, TerminalEvent::Key { event, text: Some(text) } if event.key == Key::Character('中') && text == "中")
    );
    let control = normalize_event(Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
    )));
    assert!(matches!(control, TerminalEvent::Key { text: None, .. }));
    let release = normalize_event(Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    )));
    assert!(
        matches!(release, TerminalEvent::Key { event, text: None } if event.kind == KeyKind::Release)
    );
}

#[test]
fn unsupported_modifiers_are_not_silently_converted_to_text() {
    let event = normalize_event(Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::HYPER,
    )));
    assert!(matches!(event, TerminalEvent::Unsupported(_)));
    assert!(
        matches!(normalize_event(Event::Paste("a\nb".into())), TerminalEvent::Component(ComponentEvent::Paste(text)) if text == "a\nb")
    );
}

#[test]
fn default_event_dispatch_tabs_and_exits_only_on_unhandled_escape_or_pressed_ctrl_c() {
    let mut runtime = Runtime::new();
    let root = runtime.mount_root(Box::new(Container::new())).unwrap();
    let input = runtime.append_child(root, Box::new(Input::new())).unwrap();
    let list = runtime
        .append_child(
            root,
            Box::new(SelectList::new(vec![SelectItem::new("a", "a")], 1).on_cancel(|| {})),
        )
        .unwrap();
    let mut pointer = PointerTracker::default();
    let options = RunOptions::default();
    let event = |key| TerminalEvent::Key {
        event: support::key(key),
        text: None,
    };
    dispatch_terminal_event(&mut runtime, event(Key::Tab), None, &mut pointer, options).unwrap();
    assert_eq!(runtime.focused_handle(), Some(input));
    dispatch_terminal_event(&mut runtime, event(Key::Tab), None, &mut pointer, options).unwrap();
    assert_eq!(runtime.focused_handle(), Some(list));
    assert_eq!(
        dispatch_terminal_event(
            &mut runtime,
            event(Key::Escape),
            None,
            &mut pointer,
            options
        )
        .unwrap(),
        LoopControl::Continue
    );
    runtime.set_focus(Some(input)).unwrap();
    assert_eq!(
        dispatch_terminal_event(
            &mut runtime,
            event(Key::Escape),
            None,
            &mut pointer,
            options
        )
        .unwrap(),
        LoopControl::Exit
    );
    let mut ctrl_c = support::key(Key::Character('c'));
    ctrl_c.modifiers.control = true;
    ctrl_c.kind = KeyKind::Release;
    assert_eq!(
        dispatch_terminal_event(
            &mut runtime,
            TerminalEvent::Key {
                event: ctrl_c,
                text: None
            },
            None,
            &mut pointer,
            options
        )
        .unwrap(),
        LoopControl::Continue
    );
    ctrl_c.kind = KeyKind::Press;
    assert_eq!(
        dispatch_terminal_event(
            &mut runtime,
            TerminalEvent::Key {
                event: ctrl_c,
                text: None
            },
            None,
            &mut pointer,
            options
        )
        .unwrap(),
        LoopControl::Exit
    );
}

#[test]
fn resize_requests_new_layout_and_zero_inline_height_is_rejected() {
    let mut runtime = Runtime::new();
    runtime.take_redraw_request();
    dispatch_terminal_event(
        &mut runtime,
        TerminalEvent::Resize(Size {
            width: 20,
            height: 10,
        }),
        None,
        &mut PointerTracker::default(),
        RunOptions::default(),
    )
    .unwrap();
    assert!(runtime.take_redraw_request());
    assert!(bi_tui::terminal::TerminalSession::main_screen(0).is_err());
}
