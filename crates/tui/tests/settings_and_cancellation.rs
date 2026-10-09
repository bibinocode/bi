mod support;
use bi_tui::{
    component::Component,
    protocol::{ComponentEvent, Key, MouseButton, PointerEvent, PointerKind},
    runtime::Runtime,
    terminal::{LoopControl, RunOptions, TerminalEvent, dispatch_terminal_event},
    widgets::{CancellableLoader, CancellationToken, Loader, SettingItem, SettingsList},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use support::{Host, context, key};

#[test]
fn settings_cycle_unknown_values_and_notify_only_actual_changes() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&changes);
    let mut settings = SettingsList::new(
        vec![SettingItem::new("mode", "模式", "unknown").with_values(["a", "b"])],
        3,
    )
    .unwrap()
    .on_change(move |id, value| log.borrow_mut().push((id.to_owned(), value.to_owned())));
    let mut host = Host::default();
    for key_code in [Key::Enter, Key::Character(' '), Key::Enter] {
        settings
            .handle_event(&ComponentEvent::Key(key(key_code)), &mut host)
            .unwrap();
    }
    assert_eq!(
        &*changes.borrow(),
        &[
            ("mode".into(), "a".into()),
            ("mode".into(), "b".into()),
            ("mode".into(), "a".into())
        ]
    );
    assert!(settings.update_value("mode", "外部更新").unwrap());
    assert_eq!(changes.borrow().len(), 3);
    assert!(!settings.update_value("missing", "a").unwrap());
}

#[test]
fn settings_search_backspace_and_escape_do_not_cancel_prematurely() {
    let cancelled = Rc::new(Cell::new(0));
    let count = Rc::clone(&cancelled);
    let mut settings = SettingsList::new(
        vec![
            SettingItem::new("rust", "Rust 模式", "a"),
            SettingItem::new("python", "Python 模式", "b"),
        ],
        3,
    )
    .unwrap()
    .with_search(true)
    .on_cancel(move || count.set(count.get() + 1));
    let mut host = Host::default();
    settings
        .handle_event(&ComponentEvent::Text("Rust".into()), &mut host)
        .unwrap();
    assert_eq!(settings.selected_item().unwrap().id, "rust");
    settings
        .handle_event(&ComponentEvent::Key(key(Key::Escape)), &mut host)
        .unwrap();
    assert_eq!(cancelled.get(), 0);
    settings
        .handle_event(&ComponentEvent::Key(key(Key::Escape)), &mut host)
        .unwrap();
    assert_eq!(cancelled.get(), 1);
    settings.set_filter("不存在").unwrap();
    assert!(settings.selected_item().is_none());
    let snapshot = settings.layout(&context(1, Some(2)), &mut []).unwrap();
    assert_eq!(snapshot.height, 2);
}

#[test]
fn settings_readonly_and_single_value_do_not_emit_change() {
    let calls = Rc::new(Cell::new(0));
    let count = Rc::clone(&calls);
    let mut settings = SettingsList::new(
        vec![
            SettingItem::new("readonly", "只读", "a"),
            SettingItem::new("single", "单值", "b").with_values(["b"]),
        ],
        2,
    )
    .unwrap()
    .on_change(move |_, _| count.set(count.get() + 1));
    let mut host = Host::default();
    settings
        .handle_event(&ComponentEvent::Key(key(Key::Enter)), &mut host)
        .unwrap();
    settings.select_item("single");
    settings
        .handle_event(&ComponentEvent::Key(key(Key::Enter)), &mut host)
        .unwrap();
    assert_eq!(calls.get(), 0);
    assert!(
        SettingsList::new(
            vec![
                SettingItem::new("x", "a", "a"),
                SettingItem::new("x", "b", "b")
            ],
            2
        )
        .is_err()
    );
}

#[test]
fn settings_click_accounts_for_search_row_and_clipped_height() {
    let mut settings = SettingsList::new(
        vec![SettingItem::new("x", "x", "a").with_values(["a", "b"])],
        3,
    )
    .unwrap()
    .with_search(true);
    settings.layout(&context(10, Some(2)), &mut []).unwrap();
    let response = settings
        .handle_event(
            &ComponentEvent::Pointer(PointerEvent {
                column: 0,
                row: 1,
                kind: PointerKind::Click,
                button: Some(MouseButton::Left),
                modifiers: Default::default(),
                click_count: 1,
            }),
            &mut Host::default(),
        )
        .unwrap();
    assert!(response.request_focus);
    assert_eq!(settings.items()[0].current_value, "b");
    assert_eq!(
        settings.layout(&context(0, None), &mut []).unwrap().height,
        0
    );
}

#[test]
fn cancellation_consumes_first_escape_and_calls_abort_once() {
    let calls = Rc::new(Cell::new(0));
    let count = Rc::clone(&calls);
    let loader = CancellableLoader::new("工作中").on_abort(move || count.set(count.get() + 1));
    let token = loader.token();
    let mut runtime = Runtime::new();
    let handle = runtime.mount_root(Box::new(loader)).unwrap();
    runtime.set_focus(Some(handle)).unwrap();
    let mut tracker = bi_tui::runtime::PointerTracker::default();
    let event = || TerminalEvent::Key {
        event: key(Key::Escape),
        text: None,
    };
    assert_eq!(
        dispatch_terminal_event(
            &mut runtime,
            event(),
            None,
            &mut tracker,
            RunOptions::default()
        )
        .unwrap(),
        LoopControl::Continue
    );
    assert!(token.is_cancelled());
    assert_eq!(calls.get(), 1);
    assert_eq!(
        dispatch_terminal_event(
            &mut runtime,
            event(),
            None,
            &mut tracker,
            RunOptions::default()
        )
        .unwrap(),
        LoopControl::Exit
    );
    runtime.shutdown().unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn unloading_cancellable_loader_cancels_task_token_without_abort_callback() {
    let calls = Rc::new(Cell::new(0));
    let count = Rc::clone(&calls);
    let loader = CancellableLoader::from_loader(Loader::new("静态").with_frames(vec![".".into()]))
        .on_abort(move || count.set(count.get() + 1));
    let token = loader.token();
    let mut runtime = Runtime::new();
    let handle = runtime.mount_root(Box::new(loader)).unwrap();
    runtime.remove(handle).unwrap();
    assert!(token.is_cancelled());
    assert_eq!(calls.get(), 0);
    let token = CancellationToken::default();
    let clone = token.clone();
    std::thread::spawn(move || clone.cancel()).join().unwrap();
    assert!(token.is_cancelled());
}
