//! 描述发生了什么，详情描述后希望组件执行的操作。

/// 组件对指针捕获状态的请求。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PointerCapture {
    /// 保持当前捕获状态。
    #[default]
    Unchanged,

    /// 请求捕获指针。
    ///
    /// 捕获后，后续拖动和释放事件仍发送给当前组件，
    /// 即使指针已经移出组件范围。
    Acquire,

    /// 请求释放当前组件拥有的指针捕获。
    Release,
}

/// 组件处理一个事件后的响应。
///
/// 默认值表示：未处理事件，也没有任何宿主操作请求。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EventResponse {
    /// 当前事件是否已被处理。
    ///
    /// true：停止后续传播和宿主默认行为。
    /// false：允许继续传播或执行宿主默认行为。
    pub handled: bool,

    /// 请求重新布局并绘制。
    ///
    /// 宿主可以合并多个重绘请求。
    /// 组件自身的缓存仍由组件负责失效。
    pub redraw: bool,

    /// 请求当前组件获得键盘焦点。
    ///
    /// 宿主实际切换焦点后，需要通知相关组件并请求重绘。
    pub request_focus: bool,

    pub pointer_capture: PointerCapture,
}
