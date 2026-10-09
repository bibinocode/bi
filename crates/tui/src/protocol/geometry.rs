//! 定义尺寸、坐标、布局偏移和裁剪区域

use super::{ComponentError, ComponentResult};

/// 终端尺寸，单位为终端单元格。
/// 宽、高都是 u16，与终端后端的尺寸表示一致。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

impl Size {
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// 组件文档内部的位置。
///
/// column 是终端单元格列号，不是字符索引或 UTF-8 字节偏移。
/// row 是文档行号，可以超过终端高度。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Position {
    pub column: u16,
    pub row: usize,
}

/// 子组件相对于父组件的布局偏移。
///
/// 滚动视图可以使用负偏移表示滚出视口的内容。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Offset {
    pub column: i32,
    pub row: i64,
}

impl Offset {
    /// 合并父级和子级偏移。
    pub fn checked_add(self, other: Self) -> ComponentResult<Self> {
        // 检查水平偏移是否溢出
        let column =
            self.column
                .checked_add(other.column)
                .ok_or_else(|| ComponentError::InvalidLayout {
                    reason: "horizontal layout offset overflow".into(),
                })?;

        let row = self
            .row
            .checked_add(other.row)
            .ok_or_else(|| ComponentError::InvalidLayout {
                reason: "vertical layout offset overflow".into(),
            })?;

        Ok(Self { column, row })
    }
}

/// 相对于父组件的裁剪区域。
///
/// 使用半开区间：
/// 水平方向 [column, column + width)
/// 垂直方向 [row, row + height)
///
/// 宽或高为零时，该区域不显示任何内容。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClipRect {
    pub column: i32,
    pub row: i64,
    pub width: u16,
    pub height: usize,
}

impl ClipRect {
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// 将裁剪区域平移到另一坐标系。
    ///
    /// 例如：把子组件裁剪区域转换到父组件坐标系。
    pub fn translated(self, offset: Offset) -> ComponentResult<Self> {
        let origin = Offset {
            column: self.column,
            row: self.row,
        }
        .checked_add(offset)?;

        Ok(Self {
            column: origin.column,
            row: origin.row,
            width: self.width,
            height: self.height,
        })
    }
}
