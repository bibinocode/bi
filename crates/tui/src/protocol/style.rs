//! 定义颜色和文本样式。它只描述显示效果，转换成 Ratatui 样式或终端输出的工作留给渲染层

/// 结构化颜色值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    /// 终端调色板索引，范围为 0–255。
    ///
    /// 索引 0–15 的实际颜色取决于用户的终端调色板。
    Indexed(u8),

    /// sRGB 颜色，各通道范围为 0–255。
    Rgb(u8, u8, u8),
}

/// 一段文本的完整样式。
///
/// None 表示终端默认颜色，不表示继承上一段颜色。
/// false 表示该效果关闭，不表示保留上一段效果。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub foreground: Option<Color>,
    pub background: Option<Color>,

    pub bold: bool,
    /// 用于表达终端的淡化显示效果
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub reversed: bool,
    pub crossed_out: bool,
}
