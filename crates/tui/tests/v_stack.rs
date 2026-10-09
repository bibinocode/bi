mod support;
use bi_tui::{
    component::Component,
    layout::flatten_layout,
    protocol::{ClipRect, Offset},
    widgets::{HStack, Input, Spacer, StackAlign, StackHeight, Text, VStack},
};
use support::{context, node};

#[test]
fn natural_height_and_gap_match_vertical_document_layout() {
    let mut children = [node(1, Text::new("中文中文")), node(2, Spacer::new(2))];
    let snapshot = VStack::new()
        .with_gap(1)
        .layout(&context(4, None), &mut children)
        .unwrap();
    assert_eq!(snapshot.height, 5);
    assert_eq!(snapshot.children[1].offset.row, 3);
    assert_eq!(snapshot.children[0].node.snapshot.height, 2);
}

#[test]
fn fixed_and_weighted_heights_allocate_viewports_and_padding() {
    let mut children = [
        node(1, Text::new("header")),
        node(2, Spacer::new(20)),
        node(3, Spacer::new(20)),
    ];
    let mut stack = VStack::new().with_gap(1).with_heights([
        StackHeight::Fixed(2),
        StackHeight::Fill(1),
        StackHeight::Fill(2),
    ]);
    let snapshot = stack.layout(&context(12, Some(14)), &mut children).unwrap();
    assert_eq!(snapshot.height, 14);
    assert_eq!(
        snapshot
            .children
            .iter()
            .map(|p| p.clip.unwrap().height)
            .collect::<Vec<_>>(),
        [2, 3, 7]
    );
    assert_eq!(
        snapshot
            .children
            .iter()
            .map(|p| p.offset.row)
            .collect::<Vec<_>>(),
        [0, 3, 7]
    );
    assert_eq!(snapshot.children[1].node.snapshot.height, 3);
    assert_eq!(snapshot.children[0].node.snapshot.height, 1);
}

#[test]
fn unlimited_fill_uses_content_and_zero_weight_stays_hidden() {
    let mut children = [
        node(1, Spacer::new(3)),
        node(2, Spacer::new(20)),
        node(3, Text::new("a")),
    ];
    let snapshot = VStack::new()
        .with_heights([
            StackHeight::Fill(1),
            StackHeight::Fill(0),
            StackHeight::Fixed(4),
        ])
        .layout(&context(10, None), &mut children)
        .unwrap();
    assert_eq!(snapshot.height, 7);
    assert_eq!(snapshot.children[1].node.snapshot.height, 0);
    assert_eq!(snapshot.children[2].offset.row, 3);
}

#[test]
fn horizontal_alignment_uses_child_width_and_wraps_before_alignment() {
    for (align, x) in [
        (StackAlign::Start, 0),
        (StackAlign::Center, 3),
        (StackAlign::End, 6),
    ] {
        let mut children = [node(1, Text::new("中文中文"))];
        let snapshot = VStack::new()
            .with_widths([4])
            .with_align(align)
            .layout(&context(10, None), &mut children)
            .unwrap();
        assert_eq!(snapshot.children[0].offset.column, x);
        assert_eq!(snapshot.children[0].node.snapshot.height, 2);
        assert_eq!(snapshot.children[0].clip.unwrap().column, x);
    }
}

#[test]
fn nested_horizontal_stack_receives_fill_height_and_keeps_cursor_identity() {
    let mut row = node(2, HStack::new());
    let mut input = node(3, Input::new());
    input.set_focused(true);
    row.add_child(input);
    let mut root = node(
        1,
        VStack::new().with_heights([StackHeight::Fixed(2), StackHeight::Fill(1)]),
    );
    root.add_child(node(4, Text::new("header")));
    root.add_child(row);
    let layout = root.layout(&context(10, Some(6))).unwrap();
    let flat = flatten_layout(
        &layout,
        Offset::default(),
        ClipRect {
            column: 0,
            row: 0,
            width: 10,
            height: 6,
        },
    )
    .unwrap();
    let input = flat.iter().find(|n| n.handle.id.0 == 3).unwrap();
    assert_eq!(input.offset.row, 2);
    assert!(input.snapshot.cursor.is_some());
}

#[test]
fn small_viewports_and_large_gaps_keep_clips_inside_parent() {
    for height in 0..12 {
        for gap in [0, 1, 20, u16::MAX] {
            let mut root = node(
                1,
                VStack::new().with_gap(gap).with_heights([
                    StackHeight::Auto,
                    StackHeight::Fill(1),
                    StackHeight::Fixed(5),
                ]),
            );
            root.add_child(node(2, Spacer::new(4)));
            root.add_child(node(3, Spacer::new(20)));
            root.add_child(node(4, Spacer::new(20)));
            let layout = root.layout(&context(10, Some(height))).unwrap();
            assert!(layout.snapshot.height <= usize::from(height));
            for p in &layout.snapshot.children {
                let clip = p.clip.unwrap();
                assert!(clip.row >= 0);
                assert!(clip.row as usize + clip.height <= layout.snapshot.height);
            }
            flatten_layout(
                &layout,
                Offset::default(),
                ClipRect {
                    column: 0,
                    row: 0,
                    width: 10,
                    height: usize::from(height),
                },
            )
            .unwrap();
        }
    }
    assert_eq!(
        VStack::new()
            .with_gap(20)
            .layout(&context(10, None), &mut [])
            .unwrap()
            .height,
        0
    );
}
