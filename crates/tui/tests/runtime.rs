mod support;
use bi_tui::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{
        ClipRect, ComponentError, ComponentEvent, ComponentHost, ComponentResult, EventResponse,
        Key, KeyKind, LayoutContext, MessageSender, MouseButton, Offset, PointerCapture,
        PointerEvent, PointerKind,
    },
    runtime::{PointerTracker, Runtime},
    widgets::{BoxContainer, Container, Input, Padding, Text},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use support::{context, key};

#[derive(Clone, Default)]
struct ProbeState {
    events: Rc<RefCell<Vec<ComponentEvent>>>,
    sender: Rc<RefCell<Option<MessageSender>>>,
    cleaned: Rc<Cell<usize>>,
}
struct Probe {
    state: ProbeState,
    fail_mount: bool,
    panic_unmount: bool,
    capture: bool,
}
impl Probe {
    fn new(state: &ProbeState) -> Self {
        Self {
            state: state.clone(),
            fail_mount: false,
            panic_unmount: false,
            capture: false,
        }
    }
}
impl Component for Probe {
    fn mount(&mut self, host: &mut dyn ComponentHost) -> ComponentResult<()> {
        *self.state.sender.borrow_mut() = Some(host.message_sender());
        let cleaned = Rc::clone(&self.state.cleaned);
        host.register_cleanup(Box::new(move || cleaned.set(cleaned.get() + 1)))?;
        if self.fail_mount {
            return Err(ComponentError::OperationFailed {
                message: "mount rejected".into(),
            });
        }
        Ok(())
    }
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        Text::new("probe").layout(context, children)
    }
    fn handle_event(
        &mut self,
        event: &ComponentEvent,
        _host: &mut dyn ComponentHost,
    ) -> ComponentResult<EventResponse> {
        self.state.events.borrow_mut().push(event.clone());
        let pointer_capture = match event {
            ComponentEvent::Pointer(event) if self.capture && event.kind == PointerKind::Press => {
                PointerCapture::Acquire
            }
            ComponentEvent::Pointer(event)
                if self.capture && event.kind == PointerKind::Release =>
            {
                PointerCapture::Release
            }
            _ => PointerCapture::Unchanged,
        };
        Ok(EventResponse {
            handled: !matches!(event, ComponentEvent::Key(_)),
            redraw: true,
            pointer_capture,
            ..EventResponse::default()
        })
    }
    fn unmount(&mut self, _host: &mut dyn ComponentHost) -> ComponentResult<()> {
        if self.panic_unmount {
            panic!("test unmount panic");
        }
        Ok(())
    }
}

#[test]
fn keyboard_routes_key_before_text_and_skips_release() {
    let state = ProbeState::default();
    let mut runtime = Runtime::new();
    let handle = runtime.mount_root(Box::new(Probe::new(&state))).unwrap();
    runtime.set_focus(Some(handle)).unwrap();
    state.events.borrow_mut().clear();
    assert!(
        runtime
            .dispatch_key(key(Key::Character('a')), Some("a".into()))
            .unwrap()
    );
    assert!(matches!(state.events.borrow()[0], ComponentEvent::Key(_)));
    assert_eq!(state.events.borrow()[1], ComponentEvent::Text("a".into()));
    let mut release = key(Key::Character('a'));
    release.kind = KeyKind::Release;
    assert!(!runtime.dispatch_key(release, Some("a".into())).unwrap());
    assert_eq!(state.events.borrow().len(), 2);
}

#[test]
fn stale_source_messages_are_discarded_and_queue_budget_is_bounded() {
    let old = ProbeState::default();
    let new = ProbeState::default();
    let mut runtime = Runtime::new();
    let handle = runtime.mount_root(Box::new(Probe::new(&old))).unwrap();
    let sender = old.sender.borrow().as_ref().unwrap().clone();
    let replacement = runtime.replace(handle, Box::new(Probe::new(&new))).unwrap();
    sender.post(replacement, "stale", vec![]).unwrap();
    new.sender
        .borrow()
        .as_ref()
        .unwrap()
        .post(replacement, "fresh", vec![1])
        .unwrap();
    assert_eq!(runtime.drain_messages(1).unwrap(), 1);
    assert!(new.events.borrow().is_empty());
    assert_eq!(runtime.drain_messages(1).unwrap(), 1);
    assert_eq!(
        new.events.borrow()[0],
        ComponentEvent::Message {
            topic: "fresh".into(),
            payload: vec![1]
        }
    );
    assert_eq!(old.cleaned.get(), 1);
}

#[test]
fn failed_replacement_keeps_original_and_releases_candidate_resources() {
    let old = ProbeState::default();
    let failed = ProbeState::default();
    let next = ProbeState::default();
    let mut runtime = Runtime::new();
    let handle = runtime.mount_root(Box::new(Probe::new(&old))).unwrap();
    let mut candidate = Probe::new(&failed);
    candidate.fail_mount = true;
    assert!(runtime.replace(handle, Box::new(candidate)).is_err());
    assert!(runtime.is_active(handle));
    assert_eq!(failed.cleaned.get(), 1);
    assert_eq!(old.cleaned.get(), 0);
    let replacement = runtime
        .replace(handle, Box::new(Probe::new(&next)))
        .unwrap();
    assert_eq!(replacement.generation.0, 2);
    assert_eq!(old.cleaned.get(), 1);
    runtime.shutdown().unwrap();
    assert_eq!(next.cleaned.get(), 1);
}

#[test]
fn input_replacement_restores_text_cursor_and_focus() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let submitted = Rc::clone(&values);
    let mut runtime = Runtime::new();
    let handle = runtime.mount_root(Box::new(Input::new())).unwrap();
    runtime.set_focus(Some(handle)).unwrap();
    runtime.dispatch_paste("中文abc".into()).unwrap();
    runtime.dispatch_key(key(Key::Left), None).unwrap();
    let replacement = runtime
        .replace(
            handle,
            Box::new(
                Input::new().on_submit(move |value| submitted.borrow_mut().push(value.to_owned())),
            ),
        )
        .unwrap();
    assert_eq!(runtime.focused_handle(), Some(replacement));
    runtime
        .dispatch_key(key(Key::Character('X')), Some("X".into()))
        .unwrap();
    runtime.dispatch_key(key(Key::Enter), None).unwrap();
    assert_eq!(&*values.borrow(), &["中文abXc"]);
}

#[test]
fn replacement_preserves_children_and_removal_clears_descendant_focus() {
    let mut runtime = Runtime::new();
    let root = runtime.mount_root(Box::new(Container::new())).unwrap();
    let input = runtime.append_child(root, Box::new(Input::new())).unwrap();
    runtime.set_focus(Some(input)).unwrap();
    let next = runtime.replace(root, Box::new(Container::new())).unwrap();
    assert!(runtime.is_active(input));
    assert_eq!(
        runtime
            .layout(&context(20, None))
            .unwrap()
            .unwrap()
            .snapshot
            .children
            .len(),
        1
    );
    runtime.remove(next).unwrap();
    assert!(!runtime.is_active(input));
    assert_eq!(runtime.focused_handle(), None);
}

#[test]
fn unmount_panic_does_not_skip_cleanup_or_other_nodes() {
    let first = ProbeState::default();
    let second = ProbeState::default();
    let mut runtime = Runtime::new();
    let root = runtime.mount_root(Box::new(Container::new())).unwrap();
    let mut probe = Probe::new(&first);
    probe.panic_unmount = true;
    let handle = runtime.append_child(root, Box::new(probe)).unwrap();
    runtime
        .append_child(root, Box::new(Probe::new(&second)))
        .unwrap();
    assert!(runtime.shutdown().is_err());
    assert_eq!((first.cleaned.get(), second.cleaned.get()), (1, 1));
    assert!(!runtime.is_active(handle));
    assert!(runtime.root_handle().is_none());
    runtime.shutdown().unwrap();
    assert_eq!((first.cleaned.get(), second.cleaned.get()), (1, 1));
}

#[test]
fn pointer_coordinates_and_capture_work_outside_viewport() {
    let state = ProbeState::default();
    let mut probe = Probe::new(&state);
    probe.capture = true;
    let mut runtime = Runtime::new();
    let root = runtime
        .mount_root(Box::new(BoxContainer::new(Padding::symmetric(1, 2))))
        .unwrap();
    runtime.append_child(root, Box::new(probe)).unwrap();
    let layout = runtime.layout(&context(20, None)).unwrap().unwrap();
    let viewport = ClipRect {
        column: 3,
        row: 4,
        width: 20,
        height: 10,
    };
    let offset = Offset { column: 3, row: 4 };
    let pointer = PointerEvent {
        column: 5,
        row: 5,
        kind: PointerKind::Press,
        button: Some(MouseButton::Left),
        modifiers: Default::default(),
        click_count: 0,
    };
    runtime
        .dispatch_pointer(pointer, &layout, offset, viewport)
        .unwrap();
    runtime
        .dispatch_pointer(
            PointerEvent {
                column: 0,
                row: 0,
                kind: PointerKind::Drag,
                ..pointer
            },
            &layout,
            offset,
            viewport,
        )
        .unwrap();
    runtime
        .dispatch_pointer(
            PointerEvent {
                column: 0,
                row: 0,
                kind: PointerKind::Release,
                ..pointer
            },
            &layout,
            offset,
            viewport,
        )
        .unwrap();
    let events = state.events.borrow();
    assert!(
        matches!(events[0], ComponentEvent::Pointer(event) if event.column == 0 && event.row == 0)
    );
    assert!(
        matches!(events[1], ComponentEvent::Pointer(event) if event.column == -5 && event.row == -5)
    );
    drop(events);
    assert!(
        !runtime
            .dispatch_pointer(
                PointerEvent {
                    column: 0,
                    row: 0,
                    kind: PointerKind::Move,
                    ..pointer
                },
                &layout,
                offset,
                viewport
            )
            .unwrap()
    );
}

#[test]
fn tracker_synthesizes_clicks_but_never_clicks_after_drag() {
    let pointer = PointerEvent {
        column: 2,
        row: 1,
        kind: PointerKind::Press,
        button: Some(MouseButton::Left),
        modifiers: Default::default(),
        click_count: 0,
    };
    let mut tracker = PointerTracker::default();
    tracker.process(pointer);
    assert_eq!(
        tracker.process(PointerEvent {
            kind: PointerKind::Release,
            ..pointer
        })[1]
            .click_count,
        1
    );
    tracker.process(pointer);
    assert_eq!(
        tracker.process(PointerEvent {
            kind: PointerKind::Release,
            ..pointer
        })[1]
            .click_count,
        2
    );
    tracker.process(pointer);
    tracker.process(PointerEvent {
        kind: PointerKind::Drag,
        ..pointer
    });
    assert_eq!(
        tracker
            .process(PointerEvent {
                kind: PointerKind::Release,
                ..pointer
            })
            .len(),
        1
    );
}

#[test]
fn focus_traversal_skips_labels_and_wraps_both_directions() {
    let mut runtime = Runtime::new();
    let root = runtime.mount_root(Box::new(Container::new())).unwrap();
    runtime
        .append_child(root, Box::new(Text::new("label")))
        .unwrap();
    let first = runtime.append_child(root, Box::new(Input::new())).unwrap();
    let last = runtime.append_child(root, Box::new(Input::new())).unwrap();
    runtime.focus_next(false).unwrap();
    assert_eq!(runtime.focused_handle(), Some(first));
    runtime.focus_next(true).unwrap();
    assert_eq!(runtime.focused_handle(), Some(last));
    runtime.focus_next(false).unwrap();
    assert_eq!(runtime.focused_handle(), Some(first));
}

#[test]
fn loader_ticks_through_messages_and_unloads_without_waiting_for_next_interval() {
    use bi_tui::widgets::Loader;
    use std::time::{Duration, Instant};
    let mut runtime = Runtime::new();
    let handle = runtime
        .mount_root(Box::new(
            Loader::new("工作中")
                .with_frames(vec!["a".into(), "b".into()])
                .with_interval(Duration::from_millis(10))
                .unwrap(),
        ))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while runtime.drain_messages(1).unwrap() == 0 {
        assert!(Instant::now() < deadline, "loader did not post a tick");
        std::thread::sleep(Duration::from_millis(5));
    }
    let layout = runtime.layout(&context(20, None)).unwrap().unwrap();
    assert_eq!(layout.snapshot.lines[0].spans[0].text, "b 工作中");
    runtime.remove(handle).unwrap();
    assert!(!runtime.is_active(handle));
    runtime.drain_messages(64).unwrap();
}

#[test]
fn failed_state_restore_keeps_old_input_and_focus() {
    let state = ProbeState::default();
    let mut runtime = Runtime::new();
    let handle = runtime.mount_root(Box::new(Input::new())).unwrap();
    runtime.set_focus(Some(handle)).unwrap();
    assert!(
        runtime
            .replace(handle, Box::new(Probe::new(&state)))
            .is_err()
    );
    assert_eq!(runtime.focused_handle(), Some(handle));
    assert!(runtime.is_active(handle));
    assert!(state.sender.borrow().is_none());
}

#[test]
fn committed_replacement_stays_active_even_when_old_unmount_panics() {
    let old = ProbeState::default();
    let new = ProbeState::default();
    let mut runtime = Runtime::new();
    let mut probe = Probe::new(&old);
    probe.panic_unmount = true;
    let current = runtime.mount_root(Box::new(probe)).unwrap();
    assert!(
        runtime
            .replace(current, Box::new(Probe::new(&new)))
            .is_err()
    );
    let active = runtime.active_handle(current.id).unwrap();
    assert_ne!(active, current);
    assert!(runtime.is_active(active));
    assert_eq!(old.cleaned.get(), 1);
    runtime.shutdown().unwrap();
    assert_eq!(new.cleaned.get(), 1);
}
