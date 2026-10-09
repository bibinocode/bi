use std::collections::HashMap;

use crate::protocol::{ComponentError, ComponentHandle, ComponentId, ComponentResult, Generation};

#[derive(Debug)]
struct Entry {
    active: Option<Generation>,
    pending: Option<Generation>,

    // 包括已经失败的候选实例。
    last_reserved: Generation,
}

/// 单个运行时内的组件身份注册表。
///
/// 同一个组件最多同时存在一个替换候选实例。
#[derive(Debug)]
pub struct ComponentRegistry {
    next_id: u64,
    entries: HashMap<ComponentId, Entry>,
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            entries: HashMap::new(),
        }
    }

    /// 分配新组件身份，暂不激活。
    ///
    /// 状态恢复和 mount 成功后，调用 commit。
    pub fn begin_create(&mut self) -> ComponentResult<ComponentHandle> {
        let next_id =
            self.next_id
                .checked_add(1)
                .ok_or_else(|| ComponentError::OperationFailed {
                    message: "component ID space exhausted".into(),
                })?;

        let id = ComponentId(self.next_id);
        self.next_id = next_id;

        let generation = Generation::INITIAL;

        self.entries.insert(
            id,
            Entry {
                active: None,
                pending: Some(generation),
                last_reserved: generation,
            },
        );

        Ok(ComponentHandle { id, generation })
    }

    /// 为当前有效实例分配一个替换候选。
    ///
    /// 分配后立即记录 generation，即使候选随后失败也不会复用。
    pub fn begin_replace(&mut self, current: ComponentHandle) -> ComponentResult<ComponentHandle> {
        let entry =
            self.entries
                .get_mut(&current.id)
                .ok_or_else(|| ComponentError::OperationFailed {
                    message: "component is not registered".into(),
                })?;

        if entry.active != Some(current.generation) {
            return Err(ComponentError::OperationFailed {
                message: "component handle is not active".into(),
            });
        }

        if entry.pending.is_some() {
            return Err(ComponentError::OperationFailed {
                message: "component already has a pending candidate".into(),
            });
        }

        let generation = entry.last_reserved.checked_next()?;

        entry.last_reserved = generation;
        entry.pending = Some(generation);

        Ok(ComponentHandle {
            id: current.id,
            generation,
        })
    }

    /// 激活候选实例，返回被替换的旧句柄。
    ///
    /// 新组件首次激活时返回 None。
    pub fn commit(
        &mut self,
        candidate: ComponentHandle,
    ) -> ComponentResult<Option<ComponentHandle>> {
        let entry =
            self.entries
                .get_mut(&candidate.id)
                .ok_or_else(|| ComponentError::OperationFailed {
                    message: "component is not registered".into(),
                })?;

        if entry.pending != Some(candidate.generation) {
            return Err(ComponentError::OperationFailed {
                message: "component handle is not the pending candidate".into(),
            });
        }

        let previous = entry.active.map(|generation| ComponentHandle {
            id: candidate.id,
            generation,
        });

        entry.active = Some(candidate.generation);
        entry.pending = None;

        Ok(previous)
    }

    /// 放弃候选实例，保留原来的有效实例。
    ///
    /// 本方法只更新身份状态，不执行资源清理。
    pub fn abort(&mut self, candidate: ComponentHandle) -> ComponentResult<()> {
        let entry =
            self.entries
                .get_mut(&candidate.id)
                .ok_or_else(|| ComponentError::OperationFailed {
                    message: "component is not registered".into(),
                })?;

        if entry.pending != Some(candidate.generation) {
            return Err(ComponentError::OperationFailed {
                message: "component handle is not the pending candidate".into(),
            });
        }

        entry.pending = None;

        // 首次创建失败，没有需要保留的有效实例。
        let remove_entry = entry.active.is_none();

        if remove_entry {
            self.entries.remove(&candidate.id);
        }

        Ok(())
    }

    /// 注销当前有效实例。
    ///
    /// 如果存在候选实例，调用方必须先取消候选并清理其资源。
    pub fn unregister(&mut self, current: ComponentHandle) -> ComponentResult<()> {
        let entry =
            self.entries
                .get(&current.id)
                .ok_or_else(|| ComponentError::OperationFailed {
                    message: "component is not registered".into(),
                })?;

        if entry.active != Some(current.generation) {
            return Err(ComponentError::OperationFailed {
                message: "component handle is not active".into(),
            });
        }

        if entry.pending.is_some() {
            return Err(ComponentError::OperationFailed {
                message: "cannot unregister with a pending candidate".into(),
            });
        }

        self.entries.remove(&current.id);

        Ok(())
    }

    pub fn is_active(&self, handle: ComponentHandle) -> bool {
        self.entries
            .get(&handle.id)
            .is_some_and(|entry| entry.active == Some(handle.generation))
    }

    /// 返回逻辑 ID 当前提交的实例，用于区分候选失败与提交后的清理失败。
    pub fn active_handle(&self, id: ComponentId) -> Option<ComponentHandle> {
        self.entries
            .get(&id)?
            .active
            .map(|generation| ComponentHandle { id, generation })
    }

    pub fn is_pending(&self, handle: ComponentHandle) -> bool {
        self.entries
            .get(&handle.id)
            .is_some_and(|entry| entry.pending == Some(handle.generation))
    }
}
