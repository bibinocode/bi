use std::collections::{BTreeMap, HashMap};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::protocol::{
    ComponentError, ComponentHandle, ComponentResult, ResourceCleanup, ResourceId,
};

use super::ComponentRegistry;

/// 按组件实例管理资源清理函数。
pub struct ResourceTable {
    next_id: u64,
    entries: HashMap<ComponentHandle, BTreeMap<u64, ResourceCleanup>>,
}

impl Default for ResourceTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceTable {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            entries: HashMap::new(),
        }
    }

    /// 有效实例和正在准备的候选实例都可以注册资源。
    ///
    /// 注册失败时立即执行 cleanup，避免资源泄漏。
    pub fn register(
        &mut self,
        registry: &ComponentRegistry,
        owner: ComponentHandle,
        cleanup: ResourceCleanup,
    ) -> ComponentResult<ResourceId> {
        if !registry.is_active(owner) && !registry.is_pending(owner) {
            return reject_registration(cleanup, "resource owner is neither active nor pending");
        }

        let Some(next_id) = self.next_id.checked_add(1) else {
            return reject_registration(cleanup, "resource ID space exhausted");
        };

        let id = ResourceId(self.next_id);
        self.next_id = next_id;

        self.entries.entry(owner).or_default().insert(id.0, cleanup);

        Ok(id)
    }

    /// 释放 owner 自己注册的一个资源。
    ///
    /// 不要求 owner 仍然有效，让旧实例能在 unmount 中释放资源。
    pub fn release(&mut self, owner: ComponentHandle, id: ResourceId) -> ComponentResult<()> {
        let resources =
            self.entries
                .get_mut(&owner)
                .ok_or_else(|| ComponentError::OperationFailed {
                    message: "resource does not belong to this instance".into(),
                })?;

        let cleanup = resources
            .remove(&id.0)
            .ok_or_else(|| ComponentError::OperationFailed {
                message: "resource does not belong to this instance".into(),
            })?;

        let remove_entry = resources.is_empty();

        if remove_entry {
            self.entries.remove(&owner);
        }

        // 先从表中移除，再调用用户清理函数，避免重复执行。
        run_cleanup(cleanup)
    }

    /// 清理指定实例的所有资源。
    ///
    /// 按资源注册顺序执行。即使发生错误，也继续清理剩余资源。
    /// 没有资源时返回成功。
    pub fn release_all(&mut self, owner: ComponentHandle) -> ComponentResult<()> {
        let Some(resources) = self.entries.remove(&owner) else {
            return Ok(());
        };

        let mut first_error = None;

        for cleanup in resources.into_values() {
            if let Err(error) = run_cleanup(cleanup) {
                first_error.get_or_insert(error);
            }
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub fn resource_count(&self, owner: ComponentHandle) -> usize {
        self.entries.get(&owner).map_or(0, BTreeMap::len)
    }
}

impl Drop for ResourceTable {
    fn drop(&mut self) {
        let entries = std::mem::take(&mut self.entries);

        for resources in entries.into_values() {
            for cleanup in resources.into_values() {
                let _ = run_cleanup(cleanup);
            }
        }
    }
}

fn run_cleanup(cleanup: ResourceCleanup) -> ComponentResult<()> {
    catch_unwind(AssertUnwindSafe(cleanup)).map_err(|_| ComponentError::OperationFailed {
        message: "resource cleanup panicked".into(),
    })
}

fn reject_registration(cleanup: ResourceCleanup, message: &str) -> ComponentResult<ResourceId> {
    let message = match run_cleanup(cleanup) {
        Ok(()) => message.to_owned(),
        Err(error) => format!("{message}; cleanup also failed: {error}"),
    };

    Err(ComponentError::OperationFailed { message })
}
