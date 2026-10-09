use crate::component::LayoutNode;
use crate::layout::{flatten_layout, hit_test};
use crate::protocol::{
    ClipRect, ComponentError, ComponentEvent, ComponentHandle, ComponentResult, MouseButton,
    Offset, PointerEvent, PointerKind,
};
use std::time::{Duration, Instant};

use super::Runtime;

/// 将同位置、同按钮、未拖动的按下/释放转换为 Click。
/// 连续点击要求按钮和位置相同，且距上次点击不超过 500 ms。
#[derive(Default)]
pub struct PointerTracker {
    pressed: Option<(PointerEvent, bool)>,
    last_click: Option<(MouseButton, i32, i64, Instant, u8)>,
}

impl PointerTracker {
    pub fn reset(&mut self) {
        self.pressed = None;
        self.last_click = None;
    }

    pub fn process(&mut self, event: PointerEvent) -> Vec<PointerEvent> {
        let mut events = vec![event];
        match event.kind {
            PointerKind::Press => self.pressed = Some((event, false)),
            PointerKind::Move | PointerKind::Drag => {
                if let Some((press, dragged)) = &mut self.pressed {
                    *dragged |= event.kind == PointerKind::Drag
                        || event.column != press.column
                        || event.row != press.row;
                }
            }
            PointerKind::Release => {
                if let Some((press, false)) = self.pressed.take()
                    && let Some(button) = event.button
                    && press.button == Some(button)
                    && press.column == event.column
                    && press.row == event.row
                {
                    let now = Instant::now();
                    let count = self
                        .last_click
                        .filter(|(old_button, x, y, at, _)| {
                            *old_button == button
                                && *x == event.column
                                && *y == event.row
                                && now.duration_since(*at) <= Duration::from_millis(500)
                        })
                        .map_or(1, |(_, _, _, _, count)| count.saturating_add(1));
                    self.last_click = Some((button, event.column, event.row, now, count));
                    events.push(PointerEvent {
                        kind: PointerKind::Click,
                        click_count: count,
                        ..event
                    });
                }
            }
            _ => {}
        }
        events
    }
}

impl Runtime {
    /// 根据布局快照分发鼠标事件。
    ///
    /// event 使用终端全局坐标。
    /// root_offset 和 viewport 必须与绘制该布局时一致。
    ///
    /// 组件收到的 PointerEvent 使用各自的局部坐标。
    pub fn dispatch_pointer(
        &mut self,
        event: PointerEvent,
        layout: &LayoutNode,
        root_offset: Offset,
        viewport: ClipRect,
    ) -> ComponentResult<bool> {
        let Some(root_handle) = self.root_handle() else {
            return Ok(false);
        };

        if layout.handle != root_handle {
            return Err(ComponentError::InvalidLayout {
                reason: "pointer layout does not match runtime root".into(),
            });
        }

        let nodes = flatten_layout(layout, root_offset, viewport)?;

        // 捕获者失效时，恢复普通命中检测。
        if let Some(handle) = self.pointer_capture
            && !self.registry.is_active(handle)
        {
            self.pointer_capture = None;
        }

        let target = match self.pointer_capture {
            Some(handle) => handle,
            None => {
                let Some(node) = hit_test(&nodes, event.column, event.row) else {
                    return Ok(false);
                };

                node.handle
            }
        };

        // 从原始布局查找路径，而不是只从可见列表查找。
        // 捕获者即使已经被裁剪，也仍然需要接收事件。
        let mut path = Vec::new();

        let found = find_layout_path(layout, target, root_offset, &mut path)?;

        if !found {
            return Err(ComponentError::InvalidLayout {
                reason: "pointer target is missing from layout".into(),
            });
        }

        // 分发前先校验整个路径，避免部分组件先收到事件，
        // 随后才发现祖先句柄已经失效。
        for (handle, _) in &path {
            if !self.registry.is_active(*handle) {
                return Err(ComponentError::InvalidLayout {
                    reason: "pointer layout contains a stale instance".into(),
                });
            }
        }

        // 从目标组件向根组件传播。
        for (handle, offset) in path.into_iter().rev() {
            let column = event.column.checked_sub(offset.column).ok_or_else(|| {
                ComponentError::InvalidLayout {
                    reason: "local pointer column overflow".into(),
                }
            })?;

            let row =
                event
                    .row
                    .checked_sub(offset.row)
                    .ok_or_else(|| ComponentError::InvalidLayout {
                        reason: "local pointer row overflow".into(),
                    })?;

            let local_event = ComponentEvent::Pointer(PointerEvent {
                column,
                row,
                ..event
            });

            let response = self.send_event(handle, &local_event)?;

            self.apply_response(handle, response, true)?;

            if response.handled {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

/// 找到目标时，path 保存根到目标的句柄及全局偏移。
///
/// 不跳过不可见节点，以支持隐藏或被裁剪组件的指针捕获。
fn find_layout_path(
    node: &LayoutNode,
    target: ComponentHandle,
    offset: Offset,
    path: &mut Vec<(ComponentHandle, Offset)>,
) -> ComponentResult<bool> {
    path.push((node.handle, offset));

    if node.handle == target {
        return Ok(true);
    }

    for placement in &node.snapshot.children {
        let child_offset = offset.checked_add(placement.offset)?;

        if find_layout_path(&placement.node, target, child_offset, path)? {
            return Ok(true);
        }
    }

    path.pop();

    Ok(false)
}
