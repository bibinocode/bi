use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use crate::component::{Component, ComponentNode, LayoutNode};
use crate::protocol::{
    ComponentError, ComponentEvent, ComponentHandle, ComponentResult, EventResponse, KeyEvent,
    KeyKind, LayoutContext, PointerCapture, QueuedMessage,
};

use super::{ComponentRegistry, RedrawState, ResourceTable, RuntimeHost};

/// 管理组件树、实例身份和运行时服务。
pub struct Runtime {
    root: Option<ComponentNode>,
    pub(super) registry: ComponentRegistry,
    resources: ResourceTable,
    redraw: RedrawState,

    sender: Sender<QueuedMessage>,
    receiver: Receiver<QueuedMessage>,

    focused: Option<ComponentHandle>,
    pub(super) pointer_capture: Option<ComponentHandle>,
    pub(super) focus_scope: Option<ComponentHandle>,
    saved_focus: Option<ComponentHandle>,
    layout_handles: Option<std::collections::HashSet<crate::protocol::ComponentId>>,
    keybindings: crate::keybindings::Keybindings,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();

        Self {
            root: None,
            registry: ComponentRegistry::new(),
            resources: ResourceTable::new(),
            redraw: RedrawState::new(),
            sender,
            receiver,
            focused: None,
            pointer_capture: None,
            focus_scope: None,
            saved_focus: None,
            layout_handles: None,
            keybindings: Default::default(),
        }
    }

    pub fn root_handle(&self) -> Option<ComponentHandle> {
        self.root.as_ref().map(|root| root.handle)
    }

    pub fn focused_handle(&self) -> Option<ComponentHandle> {
        self.focused
    }
    pub fn set_keybindings(&mut self, bindings: crate::keybindings::Keybindings) {
        self.keybindings = bindings;
    }
    /// 限制键盘、鼠标和 Tab 到指定子树；关闭后恢复此前有效焦点。
    /// 当前只允许一个模态范围，嵌套弹窗应放在此子树中。
    pub fn set_focus_scope(&mut self, scope: Option<ComponentHandle>) -> ComponentResult<()> {
        if let Some(scope) = scope {
            if self.focus_scope.is_some() {
                return Err(ComponentError::OperationFailed {
                    message: "a modal focus scope is already active".into(),
                });
            }
            if !self.is_active(scope) {
                return Err(ComponentError::OperationFailed {
                    message: "modal scope is not active".into(),
                });
            }
            self.saved_focus = self.focused;
            self.focus_scope = Some(scope);
            self.pointer_capture = None;
            self.set_focus(None)?;
            self.focus_next(false)?;
        } else if self.focus_scope.take().is_some() {
            self.pointer_capture = None;
            let saved = self.saved_focus.take().filter(|h| self.is_active(*h));
            self.set_focus(saved)?;
        }
        self.request_redraw();
        Ok(())
    }

    /// 按实例树前序遍历循环切换焦点；reverse 用于 Shift+Tab。
    pub fn focus_next(&mut self, reverse: bool) -> ComponentResult<bool> {
        fn collect(node: &ComponentNode, handles: &mut Vec<ComponentHandle>) {
            if node.component.focusable() {
                handles.push(node.handle);
            }
            for child in &node.children {
                collect(child, handles);
            }
        }
        let mut handles = Vec::new();
        if let Some(root) = &self.root {
            let root = self
                .focus_scope
                .and_then(|scope| find_node(root, scope))
                .unwrap_or(root);
            collect(root, &mut handles);
        }
        if let Some(visible) = &self.layout_handles {
            handles.retain(|handle| visible.contains(&handle.id));
        }
        if handles.is_empty() {
            return Ok(false);
        }
        let index = match handles
            .iter()
            .position(|handle| Some(*handle) == self.focused)
        {
            Some(index) if reverse => (index + handles.len() - 1) % handles.len(),
            Some(index) => (index + 1) % handles.len(),
            None if reverse => handles.len() - 1,
            None => 0,
        };
        self.set_focus(Some(handles[index]))?;
        Ok(true)
    }

    pub fn is_active(&self, handle: ComponentHandle) -> bool {
        self.registry.is_active(handle)
    }
    /// 获取带实例身份的发送端，后台任务只投递消息，不直接访问组件。
    pub fn message_sender(
        &self,
        source: ComponentHandle,
    ) -> ComponentResult<crate::protocol::MessageSender> {
        if !self.is_active(source) {
            return Err(ComponentError::OperationFailed {
                message: "message source is not active".into(),
            });
        }
        Ok(crate::protocol::MessageSender::new(
            source,
            self.sender.clone(),
        ))
    }

    pub fn active_handle(&self, id: crate::protocol::ComponentId) -> Option<ComponentHandle> {
        self.registry.active_handle(id)
    }

    /// 挂载根组件。
    pub fn mount_root(
        &mut self,
        component: Box<dyn Component>,
    ) -> ComponentResult<ComponentHandle> {
        if self.root.is_some() {
            return Err(ComponentError::OperationFailed {
                message: "runtime already has a root component".into(),
            });
        }

        let node = mount_node(
            component,
            &mut self.registry,
            &mut self.resources,
            &mut self.redraw,
            &self.sender,
        )?;

        let handle = node.handle;

        self.root = Some(node);
        self.redraw.request();

        Ok(handle)
    }

    /// 挂载子组件，并添加到父组件的末尾。
    pub fn append_child(
        &mut self,
        parent: ComponentHandle,
        component: Box<dyn Component>,
    ) -> ComponentResult<ComponentHandle> {
        if !self.registry.is_active(parent) {
            return Err(ComponentError::OperationFailed {
                message: "parent component is not active".into(),
            });
        }

        let root = self
            .root
            .as_mut()
            .ok_or_else(|| ComponentError::OperationFailed {
                message: "runtime has no root component".into(),
            })?;

        let parent_node =
            find_node_mut(root, parent).ok_or_else(|| ComponentError::OperationFailed {
                message: "parent component is not in the tree".into(),
            })?;

        let node = mount_node(
            component,
            &mut self.registry,
            &mut self.resources,
            &mut self.redraw,
            &self.sender,
        )?;

        let handle = node.handle;

        parent_node.add_child(node);

        // 子树结构变化可能影响祖先的布局缓存。
        root.invalidate_subtree();
        self.redraw.request();

        Ok(handle)
    }

    /// 生成当前组件树的布局快照。
    pub fn layout(&mut self, context: &LayoutContext) -> ComponentResult<Option<LayoutNode>> {
        let Some(root) = &mut self.root else {
            return Ok(None);
        };
        let layout = invoke_lifecycle("layout", || root.layout(context))?;
        crate::layout::validate_layout(&layout)?;
        fn collect(
            node: &LayoutNode,
            handles: &mut std::collections::HashSet<crate::protocol::ComponentId>,
        ) {
            handles.insert(node.handle.id);
            for child in &node.snapshot.children {
                collect(&child.node, handles);
            }
        }
        let mut handles = std::collections::HashSet::new();
        collect(&layout, &mut handles);
        self.layout_handles = Some(handles);
        Ok(Some(layout))
    }

    pub fn request_redraw(&mut self) {
        if let Some(root) = &mut self.root {
            root.invalidate_subtree();
        }
        self.redraw.request();
    }

    /// 最多处理 budget 条消息，防止消息生产者饿死终端输入。
    /// 旧 source 或 target 的消息被丢弃，但仍计入处理预算。
    pub fn drain_messages(&mut self, budget: usize) -> ComponentResult<usize> {
        let mut processed = 0;
        while processed < budget {
            let message = match self.receiver.try_recv() {
                Ok(message) => message,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            };
            processed += 1;
            if self.is_active(message.source) && self.is_active(message.target) {
                let response = self.send_event(
                    message.target,
                    &ComponentEvent::Message {
                        topic: message.topic,
                        payload: message.payload,
                    },
                )?;
                self.apply_response(message.target, response, true)?;
            }
        }
        Ok(processed)
    }

    /// 卸载指定节点及其子树。即使回调失败，节点也会被移除并清理资源。
    pub fn remove(&mut self, handle: ComponentHandle) -> ComponentResult<()> {
        if !self.is_active(handle) {
            return Err(ComponentError::OperationFailed {
                message: "component is not active".into(),
            });
        }
        let contains = |target| {
            self.root
                .as_ref()
                .and_then(|root| find_node(root, handle))
                .is_some_and(|node| find_node(node, target).is_some())
        };
        let clear_capture = self.pointer_capture.is_some_and(contains);
        let clear_scope = self.focus_scope.is_some_and(contains);
        if clear_scope {
            self.set_focus_scope(None)?;
        }
        let clear_focus = self.focused.is_some_and(|target| {
            self.root
                .as_ref()
                .and_then(|root| find_node(root, handle))
                .is_some_and(|node| find_node(node, target).is_some())
        });
        let focus_result = if clear_focus {
            self.set_focus(None)
        } else {
            Ok(())
        };
        if clear_capture {
            self.pointer_capture = None;
        }
        let removed = if self.root_handle() == Some(handle) {
            self.root.take()
        } else {
            self.root
                .as_mut()
                .and_then(|root| detach_child(root, handle))
        }
        .ok_or_else(|| ComponentError::OperationFailed {
            message: "component is not in the tree".into(),
        })?;
        let result = unmount_node(
            removed,
            &mut self.registry,
            &mut self.resources,
            &mut self.redraw,
            &self.sender,
        );
        self.request_redraw();
        merge_results(focus_result, result)
    }

    /// 保留逻辑 ID 和子节点，先恢复状态并挂载候选，再提交新实例。
    /// 候选失败时保留旧实例；提交后旧实例清理失败不会回滚新实例。
    pub fn replace(
        &mut self,
        current: ComponentHandle,
        mut component: Box<dyn Component>,
    ) -> ComponentResult<ComponentHandle> {
        let old = self
            .root
            .as_ref()
            .and_then(|root| find_node(root, current))
            .ok_or_else(|| ComponentError::OperationFailed {
                message: "component is not in the tree".into(),
            })?;
        let state = invoke_lifecycle("save_state", || old.component.save_state())?;
        let candidate = self.registry.begin_replace(current)?;
        let mut mounted = false;
        let prepare = (|| {
            if let Some(state) = state {
                invoke_lifecycle("restore_state", || component.restore_state(&state))?;
            }
            let mut host = RuntimeHost::new(
                candidate,
                &self.registry,
                &mut self.resources,
                &mut self.redraw,
                self.sender.clone(),
            );
            invoke_lifecycle("mount", || component.mount(&mut host))?;
            mounted = true;
            self.registry.commit(candidate)?;
            Ok(())
        })();
        if prepare.is_err() {
            let mut result = prepare;
            if mounted {
                let mut host = RuntimeHost::new(
                    candidate,
                    &self.registry,
                    &mut self.resources,
                    &mut self.redraw,
                    self.sender.clone(),
                );
                result = merge_results(
                    result,
                    invoke_lifecycle("unmount", || component.unmount(&mut host)),
                );
            }
            result = merge_results(result, self.registry.abort(candidate));
            result = merge_results(result, self.resources.release_all(candidate));
            return result.map(|()| candidate);
        }
        let node = find_node_mut(self.root.as_mut().expect("existing root"), current)
            .expect("existing node");
        let mut previous = std::mem::replace(&mut node.component, component);
        node.handle = candidate;
        let mut host = RuntimeHost::new(
            current,
            &self.registry,
            &mut self.resources,
            &mut self.redraw,
            self.sender.clone(),
        );
        let mut cleanup = invoke_lifecycle("unmount", || previous.unmount(&mut host));
        cleanup = merge_results(cleanup, self.resources.release_all(current));
        if self.pointer_capture == Some(current) {
            self.pointer_capture = None;
        }
        if self.focused == Some(current) {
            self.focused = Some(candidate);
            cleanup = merge_results(cleanup, self.notify_focus(candidate, true));
        }
        if self.focus_scope == Some(current) {
            self.focus_scope = Some(candidate);
        }
        self.request_redraw();
        cleanup.map(|()| candidate)
    }

    /// 在开始绘制前读取并清除请求。
    ///
    /// 绘制失败时，调用 request_redraw 重新请求。
    pub fn take_redraw_request(&mut self) -> bool {
        self.redraw.take_requested()
    }

    /// 暂时提供原始队列读取入口。
    ///
    /// 实际投递前必须检查 source 和 target 是否仍然有效。
    pub fn try_recv_message(&self) -> Result<QueuedMessage, TryRecvError> {
        self.receiver.try_recv()
    }

    /// 设置组件焦点；None 表示清除焦点。
    ///
    /// 状态先切换，再发送 FocusChanged 通知。
    /// 通知失败不会回滚焦点状态。
    pub fn set_focus(&mut self, target: Option<ComponentHandle>) -> ComponentResult<()> {
        if let (Some(scope), Some(target)) = (self.focus_scope, target)
            && self
                .root
                .as_ref()
                .and_then(|root| find_node(root, scope))
                .is_none_or(|node| find_node(node, target).is_none())
        {
            return Err(ComponentError::OperationFailed {
                message: "focus target is outside modal scope".into(),
            });
        }
        if let Some(handle) = target {
            if !self.registry.is_active(handle) {
                return Err(ComponentError::OperationFailed {
                    message: "focus target is not active".into(),
                });
            }

            let exists = self
                .root
                .as_mut()
                .and_then(|root| find_node_mut(root, handle))
                .is_some();

            if !exists {
                return Err(ComponentError::OperationFailed {
                    message: "focus target is not in the tree".into(),
                });
            }
        }

        if self.focused == target {
            return Ok(());
        }

        let previous = self.focused;
        self.focused = target;

        if let Some(root) = &mut self.root {
            if let Some(handle) = previous
                && let Some(node) = find_node_mut(root, handle)
            {
                node.set_focused(false);
            }

            if let Some(handle) = target
                && let Some(node) = find_node_mut(root, handle)
            {
                node.set_focused(true);
            }

            root.invalidate_subtree();
        }

        self.redraw.request();

        let mut result = Ok(());

        if let Some(handle) = previous
            && self.registry.is_active(handle)
        {
            let notification = self.notify_focus(handle, false);
            result = merge_results(result, notification);
        }

        // 即使旧组件通知失败，也继续通知新组件。
        if let Some(handle) = target {
            let notification = self.notify_focus(handle, true);
            result = merge_results(result, notification);
        }

        result
    }

    /// 先分发按键，未被消费时再提交对应文本。
    ///
    /// 返回是否有组件消费了按键或文本事件。
    pub fn dispatch_key(&mut self, event: KeyEvent, text: Option<String>) -> ComponentResult<bool> {
        let Some((event, remapped)) = self.keybindings.translate(event) else {
            return Ok(true);
        };
        let text = if remapped { None } else { text };
        let handled = self.route_focused(&ComponentEvent::Key(event))?;

        if handled || event.kind == KeyKind::Release {
            return Ok(handled);
        }

        match text {
            Some(text) if !text.is_empty() => self.route_focused(&ComponentEvent::Text(text)),
            _ => Ok(false),
        }
    }

    /// 将一次完整粘贴交给焦点组件及其祖先。
    pub fn dispatch_paste(&mut self, text: String) -> ComponentResult<bool> {
        self.route_focused(&ComponentEvent::Paste(text))
    }

    /// 卸载整棵组件树。
    ///
    /// 失焦或卸载失败，不会阻止其他组件的卸载和资源清理。
    pub fn shutdown(&mut self) -> ComponentResult<()> {
        self.focus_scope = None;
        self.saved_focus = None;
        self.layout_handles = None;
        let focus_result = self.set_focus(None);

        self.pointer_capture = None;

        let Some(root) = self.root.take() else {
            return focus_result;
        };

        let unmount_result = unmount_node(
            root,
            &mut self.registry,
            &mut self.resources,
            &mut self.redraw,
            &self.sender,
        );

        self.redraw.request();

        merge_results(focus_result, unmount_result)
    }

    /// 从焦点组件向根组件传播。
    ///
    /// 一次事件使用开始分发时确定的路径。
    fn route_focused(&mut self, event: &ComponentEvent) -> ComponentResult<bool> {
        let Some(focused) = self.focused else {
            return Ok(false);
        };

        if !self.registry.is_active(focused) {
            self.set_focus(None)?;
            return Ok(false);
        }

        let mut path = Vec::new();

        let found = self
            .root
            .as_ref()
            .is_some_and(|root| find_path(root, focused, &mut path));

        if !found {
            return Err(ComponentError::OperationFailed {
                message: "focused component is not in the tree".into(),
            });
        }
        if let Some(scope) = self.focus_scope
            && let Some(index) = path.iter().position(|h| *h == scope)
        {
            path.drain(..index);
        }

        // path 是根到目标，反序即目标到根。
        for handle in path.into_iter().rev() {
            if matches!(
                event,
                ComponentEvent::Key(key)
                    if key.kind == KeyKind::Release
            ) {
                let wants_release = self
                    .root
                    .as_mut()
                    .and_then(|root| find_node_mut(root, handle))
                    .is_some_and(|node| node.component.wants_key_release());

                if !wants_release {
                    continue;
                }
            }

            let response = self.send_event(handle, event)?;

            self.apply_response(handle, response, true)?;

            if response.handled {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// 向指定实例发送事件，不自动传播。
    pub(super) fn send_event(
        &mut self,
        handle: ComponentHandle,
        event: &ComponentEvent,
    ) -> ComponentResult<EventResponse> {
        if !self.registry.is_active(handle) {
            return Err(ComponentError::OperationFailed {
                message: "event target is not active".into(),
            });
        }

        let root = self
            .root
            .as_mut()
            .ok_or_else(|| ComponentError::OperationFailed {
                message: "runtime has no root component".into(),
            })?;

        let node = find_node_mut(root, handle).ok_or_else(|| ComponentError::OperationFailed {
            message: "event target is not in the tree".into(),
        })?;

        let result = {
            let mut host = RuntimeHost::new(
                handle,
                &self.registry,
                &mut self.resources,
                &mut self.redraw,
                self.sender.clone(),
            );

            invoke_lifecycle("handle_event", || {
                node.component.handle_event(event, &mut host)
            })
        };

        // 组件也可能通过 Host 请求重绘。
        if self.redraw.is_requested() || result.as_ref().is_ok_and(|response| response.redraw) {
            root.invalidate_subtree();
            self.redraw.request();
        }

        result
    }

    fn notify_focus(&mut self, handle: ComponentHandle, focused: bool) -> ComponentResult<()> {
        let response = self.send_event(handle, &ComponentEvent::FocusChanged(focused))?;

        // 不接受焦点通知中的新焦点请求，避免递归切换。
        self.apply_response(handle, response, false)
    }

    pub(super) fn apply_response(
        &mut self,
        handle: ComponentHandle,
        response: EventResponse,
        allow_focus_request: bool,
    ) -> ComponentResult<()> {
        if response.redraw {
            self.redraw.request();
        }

        match response.pointer_capture {
            PointerCapture::Unchanged => {}

            PointerCapture::Acquire => {
                if self.pointer_capture.is_some() && self.pointer_capture != Some(handle) {
                    return Err(ComponentError::OperationFailed {
                        message: "pointer is captured by another component".into(),
                    });
                }

                self.pointer_capture = Some(handle);
            }

            PointerCapture::Release => {
                // 一个组件不能释放另一个组件的捕获。
                if self.pointer_capture == Some(handle) {
                    self.pointer_capture = None;
                }
            }
        }

        if allow_focus_request && response.request_focus {
            self.set_focus(Some(handle))?;
        }

        Ok(())
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // 显式 shutdown 可以报告错误，Drop 只能尝试清理。
        let _ = self.shutdown();
    }
}

/// 创建并挂载一个没有子节点的组件实例。
fn mount_node(
    mut component: Box<dyn Component>,
    registry: &mut ComponentRegistry,
    resources: &mut ResourceTable,
    redraw: &mut RedrawState,
    sender: &Sender<QueuedMessage>,
) -> ComponentResult<ComponentNode> {
    let handle = registry.begin_create()?;

    let mount_result = {
        let mut host = RuntimeHost::new(handle, registry, resources, redraw, sender.clone());

        invoke_lifecycle("mount", || component.mount(&mut host))
    };

    let mounted = mount_result.is_ok();

    let mut result = mount_result.and_then(|()| registry.commit(handle).map(|_| ()));

    if result.is_err() {
        // mount 成功而提交失败时，补充执行 unmount。
        if mounted {
            let unmount_result = {
                let mut host =
                    RuntimeHost::new(handle, registry, resources, redraw, sender.clone());

                invoke_lifecycle("unmount", || component.unmount(&mut host))
            };

            result = merge_results(result, unmount_result);
        }

        result = merge_results(result, registry.abort(handle));

        result = merge_results(result, resources.release_all(handle));
    }

    result.map(|()| ComponentNode::new(handle, component))
}

/// 按子节点在前、父节点在后的顺序卸载。
fn unmount_node(
    mut node: ComponentNode,
    registry: &mut ComponentRegistry,
    resources: &mut ResourceTable,
    redraw: &mut RedrawState,
    sender: &Sender<QueuedMessage>,
) -> ComponentResult<()> {
    let mut result = Ok(());

    for child in node.children.drain(..) {
        let child_result = unmount_node(child, registry, resources, redraw, sender);

        result = merge_results(result, child_result);
    }

    // 先使句柄失效，拒绝后续遗留消息。
    result = merge_results(result, registry.unregister(node.handle));

    let unmount_result = {
        let mut host = RuntimeHost::new(node.handle, registry, resources, redraw, sender.clone());

        invoke_lifecycle("unmount", || node.component.unmount(&mut host))
    };

    result = merge_results(result, unmount_result);

    // 无论 unmount 是否成功，都清理剩余资源。
    merge_results(result, resources.release_all(node.handle))
}

fn find_node_mut(node: &mut ComponentNode, handle: ComponentHandle) -> Option<&mut ComponentNode> {
    if node.handle == handle {
        return Some(node);
    }

    for child in &mut node.children {
        if let Some(found) = find_node_mut(child, handle) {
            return Some(found);
        }
    }

    None
}

fn find_node(node: &ComponentNode, handle: ComponentHandle) -> Option<&ComponentNode> {
    if node.handle == handle {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_node(child, handle))
}

fn detach_child(node: &mut ComponentNode, handle: ComponentHandle) -> Option<ComponentNode> {
    if let Some(index) = node
        .children
        .iter()
        .position(|child| child.handle == handle)
    {
        return Some(node.children.remove(index));
    }
    node.children
        .iter_mut()
        .find_map(|child| detach_child(child, handle))
}

/// 找到目标时，path 保存根节点到目标节点的路径。
fn find_path(
    node: &ComponentNode,
    target: ComponentHandle,
    path: &mut Vec<ComponentHandle>,
) -> bool {
    path.push(node.handle);

    if node.handle == target {
        return true;
    }

    for child in &node.children {
        if find_path(child, target, path) {
            return true;
        }
    }

    path.pop();

    false
}

/// 将组件回调的展开式 panic 转换成组件错误。
fn invoke_lifecycle<T>(
    operation: &str,
    callback: impl FnOnce() -> ComponentResult<T>,
) -> ComponentResult<T> {
    match catch_unwind(AssertUnwindSafe(callback)) {
        Ok(result) => result,
        Err(_) => Err(ComponentError::OperationFailed {
            message: format!("component {operation} panicked"),
        }),
    }
}

/// 保留原始错误，并附加后续清理错误。
fn merge_results(first: ComponentResult<()>, second: ComponentResult<()>) -> ComponentResult<()> {
    match (first, second) {
        (Ok(()), result) => result,
        (Err(error), Ok(())) => Err(error),
        (Err(first), Err(second)) => Err(ComponentError::OperationFailed {
            message: format!("{first}; additionally: {second}",),
        }),
    }
}
