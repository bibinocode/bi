//! 定义屏幕模式、终端能力和布局上下文

use super::Size;

/// 当前使用的屏幕模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScreenMode {
    /// 在终端主屏幕中显示，使用终端原生滚动历史。
    Main,

    /// 在终端备用屏幕中显示，由应用管理视口和滚动。
    Alternate,
}

/// 本次可使用的图片协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageProtocol {
    Kitty,
    Iterm2,
}

/// 宿主为本次布局提供的有效终端能力。
///
/// 这些值已经考虑终端检测、用户配置和屏幕模式限制。
/// false 或 None 表示本次不可使用该能力。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub true_color: bool,
    pub hyperlinks: bool,
    pub images: Option<ImageProtocol>,
}

/// 组件生成布局快照时使用的上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutContext {
    /// 父布局分配给当前组件的宽度。
    ///
    /// 单位为终端单元格，可以为零。
    pub width: u16,

    /// 整个终端的实际尺寸。
    ///
    /// 不代表当前组件可占用的尺寸。
    pub terminal_size: Size,

    /// 父布局分配的高度上限。
    ///
    /// None：不限制文档高度。
    /// Some：当前组件的布局高度不得超过该值。
    ///
    /// 滚动视图测量完整子文档时，应给子组件传入 None，
    /// 再对生成的文档设置偏移和裁剪。
    pub available_height: Option<u16>,

    pub screen_mode: ScreenMode,
    pub capabilities: Capabilities,

    /// 当前组件实例是否拥有键盘焦点。
    ///
    /// 由宿主根据焦点目标设置，不由组件自行修改。
    pub focused: bool,
}
