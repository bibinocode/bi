/// UI 线程上的重绘请求状态。
#[derive(Debug)]
pub struct RedrawState {
    requested: bool,
}

impl Default for RedrawState {
    fn default() -> Self {
        Self::new()
    }
}

impl RedrawState {
    /// 首次进入运行时需要绘制一帧。
    pub const fn new() -> Self {
        Self { requested: true }
    }

    pub fn request(&mut self) {
        self.requested = true;
    }

    /// 读取并清除当前请求。
    ///
    /// 运行时在开始绘制前调用；绘制失败时需要重新请求。
    pub fn take_requested(&mut self) -> bool {
        std::mem::take(&mut self.requested)
    }

    pub const fn is_requested(&self) -> bool {
        self.requested
    }
}
