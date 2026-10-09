use crate::protocol::{ClipRect, ComponentHandle, Cursor, ImagePlacement, Line, Offset, Style};

/// 一个具体组件实例生成的布局结果。
///
/// handle 用于焦点定位、事件命中和实例版本检查。
/// 本结构不保存可变组件对象。
#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub handle: ComponentHandle,
    pub snapshot: LayoutSnapshot,
}

/// 子组件在父组件中的放置。
#[derive(Debug, Clone)]
pub struct ChildPlacement {
    /// 子组件原点相对于父组件的偏移。
    pub offset: Offset,

    /// 额外的裁剪区域，使用父组件坐标。
    ///
    /// None 表示不增加额外裁剪。
    /// 绘制与命中仍受父组件边界和祖先裁剪区域限制。
    pub clip: Option<ClipRect>,

    pub node: LayoutNode,
}

/// 一次布局的完整快照。
///
/// 尺寸、内容、光标和图片均使用当前组件的局部坐标。
#[derive(Debug, Clone, Default)]
pub struct LayoutSnapshot {
    /// 当前组件占用的宽度，单位为终端单元格。
    pub width: u16,

    /// 当前组件占用的高度，单位为文档行。
    ///
    /// 不直接使用 lines.len()：
    /// Spacer 可以没有文本但占用多行；
    /// ScrollView 可以显示一个比自身更高的子文档。
    pub height: usize,

    /// 如果设置，先用该样式填充组件区域。
    pub background: Option<Style>,

    /// 当前组件自己的文本内容。
    ///
    /// 从局部坐标 (0, 0) 开始，逐行排列。
    /// 不包含子组件的文本，不自动填充未提供的行。
    pub lines: Vec<Line>,

    /// 当前组件提供的文本光标位置。
    ///
    /// 最终是否使用，由宿主根据焦点和可见性决定。
    pub cursor: Option<Cursor>,

    /// 当前组件自己的图片放置。
    ///
    /// 必须在 height 中预留对应区域。
    pub images: Vec<ImagePlacement>,

    /// 子组件按顺序绘制，后面的可以覆盖前面的。
    ///
    /// 命中检测按相反顺序进行。
    pub children: Vec<ChildPlacement>,
}

impl LayoutSnapshot {
    /// 创建没有内容、占用零行的快照。
    pub fn empty(width: u16) -> Self {
        Self {
            width,
            ..Self::default()
        }
    }

    /// 为普通文本组件创建快照。
    ///
    /// 此时每个文本行占一行，高度由行数确定。
    /// 本方法不执行宽度或控制字符校验。
    pub fn from_lines(width: u16, lines: Vec<Line>) -> Self {
        Self {
            width,
            height: lines.len(),
            lines,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_lines() {
        let snapshot =
            LayoutSnapshot::from_lines(40, vec![Line::plain("第一行"), Line::plain("第二行")]);

        assert_eq!(snapshot.width, 40);
        assert_eq!(snapshot.height, 2);
    }
}
