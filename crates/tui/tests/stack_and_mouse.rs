mod support;

use bi_tui::{
    component::Component,
    layout::{flatten_layout, hit_test},
    protocol::{
        ClipRect, EventResponse, MouseButton, Offset, PointerCapture, PointerEvent, PointerKind,
    },
    runtime::Runtime,
    widgets::{HStack, Input, MouseRegion, Spacer, StackAlign, StackWidth, Text},
};
use std::{cell::RefCell, rc::Rc};
use support::{context, node};

#[test]
fn weighted_columns_use_all_remaining_space_and_keep_unicode_wrapping() {
    let mut stack = HStack::new().with_gap(1).with_widths([
        StackWidth::Fixed(3),
        StackWidth::Fill(1),
        StackWidth::Fill(2),
    ]);
    let mut children = [
        node(1, Text::new("abc")),
        node(2, Text::new("中文中文")),
        node(3, Text::new("right")),
    ];
    let snapshot = stack.layout(&context(15, None), &mut children).unwrap();
    assert_eq!(
        snapshot
            .children
            .iter()
            .map(|p| p.node.snapshot.width)
            .collect::<Vec<_>>(),
        [3, 3, 7]
    );
    assert_eq!(
        snapshot
            .children
            .iter()
            .map(|p| p.offset.column)
            .collect::<Vec<_>>(),
        [0, 4, 8]
    );
    assert_eq!(snapshot.height, 4);
    assert_eq!(snapshot.children[1].node.snapshot.lines.len(), 4);
    let snapshot = stack.layout(&context(6, None), &mut children).unwrap();
    assert_eq!(
        snapshot
            .children
            .iter()
            .map(|p| p.node.snapshot.width)
            .collect::<Vec<_>>(),
        [3, 0, 1]
    );
}

#[test]
fn alignment_and_height_limit_preserve_child_snapshots() {
    let mut children = [node(1, Text::new("one")), node(2, Spacer::new(5))];
    for (align, expected) in [
        (StackAlign::Start, 0),
        (StackAlign::Center, 1),
        (StackAlign::End, 2),
    ] {
        let snapshot = HStack::new()
            .with_align(align)
            .layout(&context(10, Some(3)), &mut children)
            .unwrap();
        assert_eq!(snapshot.height, 3);
        assert_eq!(snapshot.children[0].offset.row, expected);
        assert_eq!(snapshot.children[1].node.snapshot.height, 5);
        assert_eq!(snapshot.children[1].clip.unwrap().height, 3);
    }
}

#[test]
fn narrow_and_empty_stacks_have_valid_nonoverlapping_clips() {
    for count in 0..8 {
        for width in 0..25 {
            for gap in [0, 1, 30, u16::MAX] {
                let mut children: Vec<_> = (0..count)
                    .map(|i| node(i + 1, Text::new("中文abc")))
                    .collect();
                let root = node(
                    100,
                    HStack::new().with_gap(gap).with_widths([
                        StackWidth::Fixed(9),
                        StackWidth::Fill(0),
                        StackWidth::Fill(u16::MAX),
                    ]),
                );
                let mut root = root;
                root.children = std::mem::take(&mut children);
                let layout = root.layout(&context(width, Some(4))).unwrap();
                let viewport = ClipRect {
                    column: 0,
                    row: 0,
                    width,
                    height: 4,
                };
                let flattened = flatten_layout(&layout, Offset::default(), viewport).unwrap();
                let mut end = 0;
                for p in &layout.snapshot.children {
                    assert!(p.offset.column >= end);
                    end = p.offset.column + i32::from(p.node.snapshot.width);
                    assert!(end <= i32::from(width));
                }
                if width == 0 {
                    assert!(hit_test(&flattened, 0, 0).is_none());
                }
            }
        }
    }
}

fn pointer(column: i32, row: i64, kind: PointerKind) -> PointerEvent {
    PointerEvent {
        column,
        row,
        kind,
        button: Some(MouseButton::Left),
        modifiers: Default::default(),
        click_count: 0,
    }
}

#[test]
fn region_bubbles_local_coordinates_and_captures_outside_viewport() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let events = Rc::clone(&seen);
    let mut runtime = Runtime::new();
    let root = runtime
        .mount_root(Box::new(
            HStack::new()
                .with_widths([StackWidth::Fixed(4), StackWidth::Fill(1)])
                .with_gap(1),
        ))
        .unwrap();
    runtime
        .append_child(root, Box::new(Text::new("left")))
        .unwrap();
    let region = runtime
        .append_child(
            root,
            Box::new(MouseRegion::new(move |event, _| {
                events.borrow_mut().push(*event);
                Ok(EventResponse {
                    handled: true,
                    pointer_capture: match event.kind {
                        PointerKind::Press => PointerCapture::Acquire,
                        PointerKind::Release => PointerCapture::Release,
                        _ => PointerCapture::Unchanged,
                    },
                    ..EventResponse::default()
                })
            })),
        )
        .unwrap();
    runtime
        .append_child(region, Box::new(Text::new("click")))
        .unwrap();
    let layout = runtime.layout(&context(20, Some(4))).unwrap().unwrap();
    let viewport = ClipRect {
        column: 2,
        row: 3,
        width: 20,
        height: 4,
    };
    let offset = Offset { column: 2, row: 3 };
    assert!(
        runtime
            .dispatch_pointer(pointer(8, 3, PointerKind::Press), &layout, offset, viewport)
            .unwrap()
    );
    assert!(
        runtime
            .dispatch_pointer(pointer(-5, 9, PointerKind::Drag), &layout, offset, viewport)
            .unwrap()
    );
    assert!(
        runtime
            .dispatch_pointer(
                pointer(-5, 9, PointerKind::Release),
                &layout,
                offset,
                viewport
            )
            .unwrap()
    );
    assert!(
        !runtime
            .dispatch_pointer(pointer(-5, 9, PointerKind::Move), &layout, offset, viewport)
            .unwrap()
    );
    let seen = seen.borrow();
    assert_eq!((seen[0].column, seen[0].row), (1, 0));
    assert_eq!((seen[1].column, seen[1].row), (-12, 6));
    runtime.shutdown().unwrap();
}

#[test]
fn consumed_child_pointer_does_not_reach_region_and_keeps_child_focus() {
    let seen = Rc::new(RefCell::new(0));
    let events = Rc::clone(&seen);
    let mut runtime = Runtime::new();
    let region = runtime
        .mount_root(Box::new(MouseRegion::new(move |_, _| {
            *events.borrow_mut() += 1;
            Ok(EventResponse::default())
        })))
        .unwrap();
    let input = runtime
        .append_child(region, Box::new(Input::new()))
        .unwrap();
    let layout = runtime.layout(&context(20, None)).unwrap().unwrap();
    runtime
        .dispatch_pointer(
            pointer(1, 0, PointerKind::Press),
            &layout,
            Offset::default(),
            ClipRect {
                column: 0,
                row: 0,
                width: 20,
                height: 4,
            },
        )
        .unwrap();
    assert_eq!(*seen.borrow(), 0);
    assert_eq!(runtime.focused_handle(), Some(input));
    runtime.shutdown().unwrap();
}

#[test]
fn region_rejects_missing_or_multiple_children() {
    let mut region = MouseRegion::new(|_, _| Ok(EventResponse::default()));
    assert!(region.layout(&context(10, None), &mut []).is_err());
    assert!(
        region
            .layout(
                &context(10, None),
                &mut [node(1, Text::new("a")), node(2, Text::new("b"))]
            )
            .is_err()
    );
}
