//! 定义 Span、Line 、光标和图片防止、让组件可以表达具体绘制内容

use std::sync::Arc;

use super::{Position, Size, Style};

/// 一段具有统一样式的文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// 普通文本，不包含换行或终端控制序列。
    ///
    /// 这里保存原始文本，不进行折行或截断。
    pub text: String,

    pub style: Style,

    /// 可选的超链接目标。
    ///
    /// 是否输出终端超链接，由终端能力和渲染策略决定。
    pub hyperlink: Option<String>,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::default(),
            hyperlink: None,
        }
    }

    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            hyperlink: None,
        }
    }
}

/// 一行结构化文本。
///
/// spans 按顺序连接，不在片段之间自动添加空格。
/// 空 spans 表示空行。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            spans: vec![Span::plain(text)],
        }
    }
}

/// 当前组件文档内的文本光标位置。
///
/// 用于定位终端硬件光标，辅助输入法候选窗口定位。
/// 光标是否显示，由宿主策略决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub position: Position,
}

/// 图片在当前组件文档内的放置。
#[derive(Debug, Clone)]
pub struct ImagePlacement {
    /// 当前组件实例内的图片内容标识。
    ///
    /// 图片数据变化时应使用新的 content_id。
    /// 不同组件可以使用相同字符串，宿主必须按组件实例隔离。
    pub content_id: String,

    /// 图片数据的 MIME 类型，例如 image/png。
    pub mime_type: String,

    /// 编码后的图片文件数据。
    ///
    /// 不是原始像素数组、base64 文本或终端控制序列。
    /// Arc 使布局快照可以共享数据，避免反复复制图片。
    pub data: Arc<[u8]>,

    /// 图片左上角在组件文档中的位置。
    pub position: Position,

    /// 图片占用的终端单元格尺寸。
    pub size: Size,
}
