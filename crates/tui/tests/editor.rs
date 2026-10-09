mod support;
use bi_tui::{
    autocomplete::{AutocompleteProvider, CommandAutocomplete, Completion, SlashCommand},
    component::{Component, LayoutNode},
    protocol::{
        ComponentEvent, ComponentHandle, ComponentId, ComponentResult, Key, KeyKind, MouseButton,
        PointerEvent, PointerKind,
    },
    widgets::Editor,
};
use std::{cell::RefCell, rc::Rc};
use support::{Host, context, key};

fn press(editor: &mut Editor, key_code: Key) {
    editor
        .handle_event(&ComponentEvent::Key(key(key_code)), &mut Host::default())
        .unwrap();
}
fn large_paste() -> String {
    (1..=12)
        .map(|line| format!("原文第{line}行"))
        .collect::<Vec<_>>()
        .join("\n")
}
fn validate(editor: &mut Editor, width: u16, height: u16) {
    let snapshot = editor
        .layout(&context(width, Some(height)), &mut [])
        .unwrap();
    bi_tui::layout::validate_layout(&LayoutNode {
        handle: ComponentHandle::initial(ComponentId(1)),
        snapshot,
    })
    .unwrap();
}

#[test]
fn editor_submits_expanded_trimmed_text_and_clears_document() {
    let output = Rc::new(RefCell::new(String::new()));
    let submitted = Rc::clone(&output);
    let mut editor = Editor::new().on_submit(move |text| *submitted.borrow_mut() = text.into());
    editor.set_text("  中文").unwrap();
    let mut newline = key(Key::Enter);
    newline.modifiers.alt = true;
    editor
        .handle_event(&ComponentEvent::Key(newline), &mut Host::default())
        .unwrap();
    editor.insert_text("下一行  ").unwrap();
    press(&mut editor, Key::Enter);
    assert_eq!(&*output.borrow(), "中文\n下一行");
    assert_eq!(editor.text(), "");
    assert_eq!(editor.cursor(), 0);
}

#[test]
fn multiline_backspace_joins_lines_and_deletes_unicode_units() {
    let mut editor = Editor::new();
    editor.set_text("A\ne\u{301}👩‍💻").unwrap();
    press(&mut editor, Key::Backspace);
    assert_eq!(editor.text(), "A\ne\u{301}");
    press(&mut editor, Key::Backspace);
    assert_eq!(editor.text(), "A\n");
    press(&mut editor, Key::Backspace);
    assert_eq!(editor.text(), "A");
}

#[test]
fn vertical_motion_preserves_target_column_across_short_lines() {
    let mut editor = Editor::new();
    editor.set_text("abcdef\nx\nabcdef").unwrap();
    editor.layout(&context(20, None), &mut []).unwrap();
    press(&mut editor, Key::Up);
    assert_eq!(editor.cursor(), 8);
    press(&mut editor, Key::Up);
    assert_eq!(editor.cursor(), 6);
    press(&mut editor, Key::Down);
    press(&mut editor, Key::Down);
    assert_eq!(editor.cursor(), editor.text().len());
}

#[test]
fn exact_width_end_cursor_and_viewport_scroll_are_valid() {
    let mut editor = Editor::new().with_height(2);
    editor.set_text("中文").unwrap();
    let snapshot = editor.layout(&context(4, None), &mut []).unwrap();
    assert_eq!(snapshot.cursor.unwrap().position.row, 1);
    assert_eq!(snapshot.cursor.unwrap().position.column, 0);
    editor.set_text("第一行\n第二行\n第三行\n第四行").unwrap();
    let snapshot = editor.layout(&context(10, None), &mut []).unwrap();
    assert_eq!(snapshot.height, 2);
    assert_eq!(snapshot.cursor.unwrap().position.row, 1);
    assert_eq!(snapshot.lines[0].spans[0].text, "第三行");
}

#[test]
fn large_paste_is_atomic_delete_and_undo_restore_original_text() {
    let mut editor = Editor::new();
    editor.set_text("开头 ").unwrap();
    let original = large_paste();
    editor
        .handle_event(
            &ComponentEvent::Paste(original.clone()),
            &mut Host::default(),
        )
        .unwrap();
    assert!(editor.text().contains("[paste #1 +12 lines]"));
    assert_eq!(editor.expanded_text(), format!("开头 {original}"));
    press(&mut editor, Key::Backspace);
    assert_eq!(editor.text(), "开头 ");
    let mut undo = key(Key::Character('z'));
    undo.modifiers.control = true;
    editor
        .handle_event(&ComponentEvent::Key(undo), &mut Host::default())
        .unwrap();
    assert_eq!(editor.expanded_text(), format!("开头 {original}"));
    for width in 1..=8 {
        validate(&mut editor, width, 2);
    }
}

#[test]
fn large_paste_change_callback_never_receives_internal_marker() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&changes);
    let mut editor = Editor::new().on_change(move |text| log.borrow_mut().push(text.to_owned()));
    let original = large_paste();
    editor
        .handle_event(
            &ComponentEvent::Paste(original.clone()),
            &mut Host::default(),
        )
        .unwrap();
    assert_eq!(&*changes.borrow(), &[original]);
}

#[test]
fn literal_marker_is_not_expanded_and_multiple_pastes_keep_ranges_after_edits() {
    let mut editor = Editor::new();
    let literal = "[paste #1 +12 lines]";
    editor.set_text(literal).unwrap();
    let pasted = large_paste();
    editor
        .handle_event(&ComponentEvent::Paste(pasted.clone()), &mut Host::default())
        .unwrap();
    press(&mut editor, Key::Home);
    editor.insert_text("前缀 ").unwrap();
    assert_eq!(editor.expanded_text(), format!("前缀 {literal}{pasted}"));
    press(&mut editor, Key::End);
    editor
        .handle_event(&ComponentEvent::Paste(pasted.clone()), &mut Host::default())
        .unwrap();
    assert_eq!(
        editor.expanded_text(),
        format!("前缀 {literal}{pasted}{pasted}")
    );
    press(&mut editor, Key::Backspace);
    assert_eq!(editor.expanded_text(), format!("前缀 {literal}{pasted}"));
}

#[test]
fn editor_state_roundtrip_includes_pastes_and_rejects_corrupt_payload_atomically() {
    let mut original = Editor::new();
    original
        .handle_event(&ComponentEvent::Paste(large_paste()), &mut Host::default())
        .unwrap();
    let state = original.save_state().unwrap().unwrap();
    let mut restored = Editor::new();
    restored.restore_state(&state).unwrap();
    assert_eq!(restored.expanded_text(), original.expanded_text());
    for length in [0, 1, 7, 8, state.payload.len() - 1] {
        let mut corrupt = state.clone();
        corrupt.payload.truncate(length);
        assert!(restored.restore_state(&corrupt).is_err());
        assert_eq!(restored.expanded_text(), original.expanded_text());
    }
    let mut corrupt = state;
    corrupt.payload[..8].copy_from_slice(&1u64.to_le_bytes());
    assert!(restored.restore_state(&corrupt).is_err());
}

#[test]
fn history_navigation_restores_draft_and_paste_registry() {
    let mut editor = Editor::new();
    editor.add_to_history("旧输入").unwrap();
    editor
        .handle_event(&ComponentEvent::Paste(large_paste()), &mut Host::default())
        .unwrap();
    let draft = editor.expanded_text();
    let mut older = key(Key::Up);
    older.modifiers.alt = true;
    editor
        .handle_event(&ComponentEvent::Key(older), &mut Host::default())
        .unwrap();
    assert_eq!(editor.text(), "旧输入");
    let mut newer = key(Key::Down);
    newer.modifiers.alt = true;
    editor
        .handle_event(&ComponentEvent::Key(newer), &mut Host::default())
        .unwrap();
    assert_eq!(editor.expanded_text(), draft);
}

#[test]
fn command_completion_replaces_whole_token_and_preserves_arguments() {
    let provider = CommandAutocomplete::new(vec![SlashCommand::new("help")]).unwrap();
    let completion = provider
        .suggestions("中文\n/hexxx 参数", "中文\n/he".len())
        .unwrap();
    assert_eq!(completion[0].range, "中文\n".len().."中文\n/hexxx".len());
    let mut editor = Editor::new().with_autocomplete(provider);
    editor.set_text("/he").unwrap();
    press(&mut editor, Key::Tab);
    assert_eq!(editor.text(), "/help");
}

#[test]
fn completion_menu_accepts_selection_without_submitting_and_escape_only_dismisses() {
    let calls = Rc::new(RefCell::new(0));
    let submitted = Rc::clone(&calls);
    let provider = CommandAutocomplete::new(vec![
        SlashCommand::new("help"),
        SlashCommand::new("history"),
    ])
    .unwrap();
    let mut editor = Editor::new()
        .with_autocomplete(provider)
        .on_submit(move |_| *submitted.borrow_mut() += 1);
    editor.set_text("/h").unwrap();
    press(&mut editor, Key::Tab);
    validate(&mut editor, 5, 2);
    press(&mut editor, Key::Down);
    press(&mut editor, Key::Enter);
    assert_eq!(editor.text(), "/history");
    assert_eq!(*calls.borrow(), 0);
    editor.set_text("/h").unwrap();
    press(&mut editor, Key::Tab);
    assert!(
        editor
            .handle_event(&ComponentEvent::Key(key(Key::Escape)), &mut Host::default())
            .unwrap()
            .handled
    );
    assert!(
        !editor
            .handle_event(&ComponentEvent::Key(key(Key::Escape)), &mut Host::default())
            .unwrap()
            .handled
    );
}

#[test]
fn invalid_provider_ranges_do_not_modify_editor() {
    struct Invalid;
    impl AutocompleteProvider for Invalid {
        fn suggestions(&self, _: &str, _: usize) -> ComponentResult<Vec<Completion>> {
            Ok(vec![Completion {
                label: "bad".into(),
                replacement: "x".into(),
                description: None,
                range: 1..2,
            }])
        }
    }
    let mut editor = Editor::new().with_autocomplete(Invalid);
    editor.set_text("中文").unwrap();
    assert!(
        editor
            .handle_event(&ComponentEvent::Key(key(Key::Tab)), &mut Host::default())
            .is_err()
    );
    assert_eq!(editor.text(), "中文");
}

#[test]
fn mouse_click_places_cursor_using_unicode_columns_and_requests_focus() {
    let mut editor = Editor::new();
    editor.set_text("中文abc").unwrap();
    editor.layout(&context(10, None), &mut []).unwrap();
    let response = editor
        .handle_event(
            &ComponentEvent::Pointer(PointerEvent {
                column: 2,
                row: 0,
                kind: PointerKind::Press,
                button: Some(MouseButton::Left),
                modifiers: Default::default(),
                click_count: 0,
            }),
            &mut Host::default(),
        )
        .unwrap();
    assert_eq!(editor.cursor(), "中".len());
    assert!(response.request_focus);
}

#[test]
fn editor_releases_do_not_submit_or_edit_and_normalization_rejects_ansi() {
    let mut editor = Editor::new();
    editor.set_text("a\r\nb\tc").unwrap();
    assert_eq!(editor.text(), "a\nb    c");
    let mut release = key(Key::Enter);
    release.kind = KeyKind::Release;
    assert!(
        !editor
            .handle_event(&ComponentEvent::Key(release), &mut Host::default())
            .unwrap()
            .handled
    );
    assert!(editor.insert_text("\x1b[31m").is_err());
    assert_eq!(editor.text(), "a\nb    c");
}

#[test]
fn mixed_edits_remain_valid_at_narrow_sizes() {
    for width in 1..=9 {
        let mut editor = Editor::new();
        for step in 0..80 {
            match step % 8 {
                0 => editor.insert_text("中").unwrap(),
                1 => editor.insert_text("e\u{301}").unwrap(),
                2 => press(&mut editor, Key::Left),
                3 => editor.insert_text("👩‍💻").unwrap(),
                4 => editor.insert_text("\n").unwrap(),
                5 => press(&mut editor, Key::Up),
                6 => press(&mut editor, Key::Delete),
                _ => press(&mut editor, Key::Down),
            }
            validate(&mut editor, width, 3);
        }
    }
}

#[test]
fn paste_marker_boundary_stays_atomic_next_to_combining_mark() {
    let mut editor = Editor::new();
    editor
        .handle_event(&ComponentEvent::Paste(large_paste()), &mut Host::default())
        .unwrap();
    editor.insert_text("\u{301}").unwrap();
    press(&mut editor, Key::Left);
    let state = editor.save_state().unwrap().unwrap();
    let mut restored = Editor::new();
    restored.restore_state(&state).unwrap();
    press(&mut restored, Key::Backspace);
    assert_eq!(restored.expanded_text(), "\u{301}");
    validate(&mut restored, 1, 1);
}

#[test]
fn runtime_replacement_restores_editor_paste_registry_and_submission() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let received = Rc::clone(&values);
    let mut runtime = bi_tui::runtime::Runtime::new();
    let handle = runtime.mount_root(Box::new(Editor::new())).unwrap();
    runtime.set_focus(Some(handle)).unwrap();
    let original = large_paste();
    runtime.dispatch_paste(original.clone()).unwrap();
    let replacement = runtime
        .replace(
            handle,
            Box::new(
                Editor::new().on_submit(move |text| received.borrow_mut().push(text.to_owned())),
            ),
        )
        .unwrap();
    assert_eq!(runtime.focused_handle(), Some(replacement));
    runtime.dispatch_key(key(Key::Enter), None).unwrap();
    assert_eq!(&*values.borrow(), &[original]);
}
