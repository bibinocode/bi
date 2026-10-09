//! 组件身份和实例版本，为焦点定位、事件派发、热重载准备

use super::{ComponentError, ComponentResult};

// ComponentId：标识逻辑组件，例如某个编辑器。
// Generation：标识该组件的当前实例。热重载替换实例时递增。

/// 逻辑组件的稳定标识。
///
/// 由宿主分配，在同一个 UI 运行时中保持唯一。
/// 卸载后不应将该 ID 分配给无关组件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComponentId(pub u64);

/// 同一逻辑组件的实例版本。
///
/// 替换组件实例时递增，不允许溢出后回到零。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Generation(pub u64);

impl Generation {
    pub const INITIAL: Self = Self(0);

    pub fn checked_next(self) -> ComponentResult<Self> {
        let value = self
            .0
            .checked_add(1)
            .ok_or_else(|| ComponentError::OperationFailed {
                message: "component generation exhausted".into(),
            })?;

        Ok(Self(value))
    }
}

/// 一个具体组件实例的句柄。
///
/// 宿主处理请求前，必须同时检查 ID 和 generation。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComponentHandle {
    pub id: ComponentId,
    pub generation: Generation,
}

impl ComponentHandle {
    pub const fn initial(id: ComponentId) -> Self {
        Self {
            id,
            generation: Generation::INITIAL,
        }
    }

    /// 为替换实例生成候选句柄。
    ///
    /// 此方法不会修改宿主注册表，也不会使旧实例失效。
    /// 宿主成功提交替换后，新句柄才成为当前实例。
    pub fn next_generation(self) -> ComponentResult<Self> {
        Ok(Self {
            id: self.id,
            generation: self.generation.checked_next()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_handle_next_generation() {
        let old = ComponentHandle::initial(ComponentId(42));
        let candidate = old.next_generation().expect("next_generation failed");

        // 逻辑组件相同
        assert_eq!(old.id, candidate.id);

        // 实例版本不同。
        assert_ne!(old.generation, candidate.generation);

        // 完整句柄也不同。
        assert_ne!(old, candidate);
    }
}
