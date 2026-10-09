//! 定义组件操作统一使用的错误和返回类型

use thiserror::Error;

pub type ComponentResult<T> = Result<T, ComponentError>;

/**
 * 组件操作错误
 * example
 * ```
 * use bi_tui::protocol::{ComponentError, ComponentResult};
 * let result: ComponentResult<u32> = Ok(100);
 * let err: ComponentResult<()> = Err(ComponentError::InvalidContent { reason: "invalid content".into() });
 * println!("{:?}", err);
 * ```
 */
#[derive(Debug, Error)]
pub enum ComponentError {
    /**
     * 组件内容无效
     * @param reason 错误原因
     */
    #[error("invalid component content: {reason}")]
    InvalidContent { reason: String },

    #[error("invalid component layout: {reason}")]
    InvalidLayout { reason: String },

    #[error("unsupported component state version: {version}")]
    UnsupportedState { version: u32 },

    #[error("component operation failed: {message}")]
    OperationFailed { message: String },
}
