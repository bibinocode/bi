//! 定义组件收到的事件，Crossterm 输入转换、鼠标点击判定和事件路由由宿主负责。

/// 按键或指针事件携带的修饰键。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub super_key: bool,
}

/// 标准化后的按键标识。
///
/// Shift+Tab 表示为 Tab + shift，不单独定义 BackTab。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Character(char),
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Function(u8),
}

/// 按键事件类型。
///
/// 终端未提供重复或释放信息时，输入层不能凭空生成它们。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyKind {
    Press,
    Repeat,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
    pub kind: KeyKind,

    /// 输入协议提供的标准键盘布局键。
    ///
    /// 用于非拉丁布局下的快捷键匹配，不是硬件扫描码。
    /// 后端无法提供时为 None。
    pub base_layout_key: Option<Key>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Move,
    Press,
    Release,
    Drag,

    /// 宿主确认按下、释放之间没有发生拖动后生成。
    Click,

    /// 标准化后的滚动量，单位为终端单元格。
    ///
    /// columns：正数向右，负数向左。
    /// rows：正数向下，负数向上。
    Scroll {
        columns: i32,
        rows: i32,
    },
}

/// 派发给组件的局部指针事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerEvent {
    /// 相对于当前组件的单元格坐标。
    ///
    /// 指针捕获期间允许位于组件外，因此使用有符号值。
    pub column: i32,
    pub row: i64,

    pub kind: PointerKind,

    /// 移动或滚动事件可以没有关联按钮。
    pub button: Option<MouseButton>,

    pub modifiers: Modifiers,

    /// Click 事件的连续点击次数。
    /// 非 Click 事件为零。
    pub click_count: u8,
}

/// 组件接收的标准化事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentEvent {
    /// 按键动作，用于快捷键、导航和编辑命令。
    Key(KeyEvent),

    /// 已提交的输入文本，可以包含多个字符。
    ///
    /// 不表示输入法预编辑状态。
    /// 不携带文本样式或终端控制序列。
    Text(String),

    /// 一次完整粘贴。
    ///
    /// 保留多行内容，由输入组件处理换行和大段粘贴。
    Paste(String),

    Pointer(PointerEvent),

    /// 当前组件的键盘焦点变化。
    ///
    /// 不是终端窗口获得或失去焦点。
    FocusChanged(bool),

    /// 异步任务或其他组件投递的消息。
    ///
    /// 宿主检查目标 ComponentHandle 有效后才派发。
    Message {
        topic: String,
        payload: Vec<u8>,
    },
}
