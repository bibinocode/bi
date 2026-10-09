# bi-tui

Rust 终端 UI 库，使用 Ratatui 绘制结构化布局、Crossterm 管理终端会话与输入，不依赖 AI 模块。当前已完成基础运行时和常用交互组件；TS 参考范围及实现边界见 [运行时说明](../../docs/tui-runtime.md)。

## 运行示例

在 workspace 根目录执行：

```powershell
cargo run -p bi-tui --example interactive
```

默认使用备用屏幕。Tab/Shift+Tab 切换输入框、列表和滚动区域的焦点，也可以用鼠标点击切换。输入框 Enter 提交，列表上下键选择、Enter 确认，滚动区域支持滚轮和 PageUp/PageDown。未被组件消费的 Escape 或 Ctrl+C 退出；退出会卸载组件、取消 Loader 定时线程并恢复终端会话。

主屏幕固定高度内联视口：

```powershell
cargo run -p bi-tui --example interactive -- --main
```

不需要真实终端的验证示例：

```powershell
cargo run -p bi-tui --example headless
cargo run -p bi-tui --example registry
```

`headless` 使用 TestBackend，派发中文粘贴和提交事件，输出绘制结果与焦点光标。`registry` 检查候选失败、generation 递增和实例提交。另有 `render_text` 展示主屏幕文本渲染与窗口尺寸变化，按任意键退出。

多行编辑、命令补全、设置和可取消任务示例：

```powershell
cargo run -p bi-tui --example editor
cargo run -p bi-tui --example editor -- --main
cargo run -p bi-tui --example editor -- --headless
```

`--headless` 使用 TestBackend 脚本执行命令补全、多行提交、设置修改和取消，不启用真实终端输入模式。

横向布局与可点击文本示例：

```powershell
cargo run -p bi-tui --example stack
cargo run -p bi-tui --example stack -- --main
cargo run -p bi-tui --example stack -- --headless
```

`stack` 将编辑区和设置区按 2:1 分配剩余宽度，支持 Tab 切换焦点、调整窗口大小和点击右栏文本。`--headless` 验证鼠标命中与回调，并输出 TestBackend 绘制结果。

## 已有组件

| 组件 | 功能 |
| --- | --- |
| `Container` | 子组件纵向排列，保留完整子快照，按容器边界裁剪 |
| `HStack` / `StackWidth` / `StackAlign` | 横向排列、固定与按权重分配的宽度、间距、纵向起始/居中/末尾对齐 |
| `VStack` / `StackHeight` | 纵向排列、自然/固定/按权重分配的高度、行间距、子项宽度与横向对齐 |
| `MouseRegion` | 单子节点鼠标包装，子节点优先消费，未消费事件交回调，可请求指针捕获 |
| `BoxContainer` / `Padding` | 内边距、背景样式和内部裁剪 |
| `Spacer` | 无文本也通过快照高度占用空行 |
| `Text` | 多行纯文本、Unicode 列宽硬换行、文本样式 |
| `TruncatedText` | 第一行按列宽截断，超宽显示省略号 |
| `Input` | 单行输入、占位文本、字素簇编辑、水平滚动、鼠标定位、提交、粘贴、撤销、状态保存 |
| `ScrollView` | 一个子组件、固定期望高度的纵向视口、跟随末尾、键盘与滚轮滚动 |
| `SelectList` / `SelectItem` | value 前缀过滤、键盘循环导航、滚轮与鼠标选择、提交和取消回调 |
| `Loader` | 定时消息驱动动画，卸载时取消并清理线程 |
| `Editor` | 多行编辑、自动换行与可见行滚动、鼠标定位、撤销、历史、大段粘贴、命令补全、状态恢复 |
| `SettingsList` / `SettingItem` | 设置值循环切换、标签搜索、只读项、鼠标选择和修改回调 |
| `CancellableLoader` / `CancellationToken` | Escape 取消、跨线程取消信号、停止动画定时器、一次性取消回调 |

`HStack::with_widths()` 的配置按当前子节点顺序对应，缺失项默认为 `StackWidth::Fill(1)`。先扣除间距，再按顺序给 `Fixed(n)` 分配最多 n 列，最后让 `Fill(weight)` 按权重分享剩余列；权重为零的子项不分配宽度。窄屏下固定项和间距会压缩其余子项，零宽项不可见，但仍保留运行时身份，也仍可能进入 Tab 遍历。配置不自动跟随子节点 ID，增删子节点时需要考虑位置变化。需要自动尺寸、伸缩约束或响应式隐藏时使用下述 `with_entries()` 接口。

`MouseRegion::new(handler)` 是运行时父节点，必须通过 `append_child()` 添加恰好一个子节点。子节点的挂载、卸载和焦点仍由运行时管理；回调仅接收沿事件路径冒泡的指针事件，不会监听其他节点已经消费的事件，也不生成鼠标进入/离开事件。

`VStack` 默认使用 `StackHeight::Auto`，按子项内容高度纵向排列。`with_gap(n)` 设置行间距，`with_heights()` 配置 `Auto`、`Fixed(n)` 或 `Fill(weight)`；有高度上限时先预留间距，按顺序分配 Auto/Fixed，再按权重分配剩余高度。没有高度上限时，正权重 Fill 使用内容高度，Fill(0) 始终占零行。固定高度大于子项内容时保留空白，小于内容时由子项按分配高度布局并裁剪。高度不足时靠后的子项可能不可见。

`VStack::with_widths([20, 30])` 设置子项宽度上限，缺失项使用父宽度；配合 `with_align(StackAlign::Center)` 或 `End` 可以横向居中或右对齐。高度和宽度配置均按当前子节点位置对应。需要自然高度的子项先测量完整内容，再在高度受限时按实际分配高度布局；因此组件的 `layout()` 应遵守已有的无 IO、无资源注册约定。`stack` 示例已使用 VStack 组织顶层内容和右栏。

Input 的快捷键：

| 按键 | 行为 |
| --- | --- |
| Left / Right、Home / End | 按字素簇移动、移至行首或行尾 |
| Backspace / Delete | 删除前一个或后一个字素簇 |
| Ctrl+A / Ctrl+E | 移至行首或行尾 |
| Ctrl+U / Ctrl+K | 删除到行首或行尾 |
| Ctrl+W / Alt+Backspace | 删除前一个单词 |
| Ctrl+Left / Ctrl+Right、Alt+Left / Alt+Right、Alt+B / Alt+F | 按空白分隔的单词移动 |
| Alt+D / Alt+Delete | 删除后一个单词 |
| Ctrl+Z | 撤销，最多保存 100 次修改 |
| Enter | 调用提交回调 |

Input 粘贴对齐 TS 单行行为：去掉 CR/LF，将 Tab 转为四个空格；其余控制字符会报错。直接设置 value 时不执行该粘贴归一化。组件 setter 不自动通知宿主，运行时接入后需要重新布局或请求重绘。

## 多行编辑与取消

Editor 默认最多显示八行（包含补全菜单），可通过 `with_height()` 配置。Enter 提交展开粘贴标记并 trim 后的文本，然后清空编辑内容；Alt+Enter、Shift+Enter 或 Ctrl+Enter 插入换行，实际按键是否可区分取决于终端。Home/End 和 Ctrl+A/Ctrl+E 移到当前逻辑行首尾；Ctrl+U/Ctrl+K 删除到当前逻辑行首尾；Ctrl+Z 撤销。Alt+Up/Alt+Down 浏览最多 100 条历史，再向下返回原草稿。普通 Up/Down 按可见换行后的行移动。

Editor 将 CRLF 和 CR 转为 LF，将 Tab 转为四个空格。超过十行的粘贴保存为原文和一个原子标记，Left/Right、Backspace/Delete 不拆开标记；撤销与状态恢复保留原文。`text()` 返回含标记的编辑文本，`expanded_text()` 与 `on_change` 回调返回展开原文；手写相同形状的标记不会被展开。只按行数压缩，不按字符数压缩。

通过 `with_autocomplete(CommandAutocomplete::new(...)? )` 配置斜杠命令。当前逻辑行开头输入 `/` 前缀，再按 Tab 请求候选；单个候选直接替换，多个候选可用 Up/Down/Tab 选择、Enter 接受、Escape 关闭，也可点击候选。没有候选时 Tab 留给宿主切换焦点。补全范围使用 UTF-8 字节索引，宿主校验范围和文本后才应用。

SettingsList 的 Enter 或空查询时的 Space 默认循环切换 `values`，空 `values` 默认为只读项；配置 `with_submenu()` 后 Enter 可打开动态候选菜单。`with_search(true)` 启用标签的不区分大小写子串搜索，Backspace 删除查询中的一个字素簇，Escape 先清除非空查询，再执行取消回调或交给宿主退出。通过 `update_value()` 修改设置不会触发用户修改回调。

CancellableLoader 需要获得焦点才能响应 Escape。第一次 Escape 设置取消信号、停止并清理动画定时线程，`on_abort` 仅执行一次；第二次 Escape 留给宿主退出。卸载也会设置取消信号，但不会调用 `on_abort`。后台任务必须自行检查 `CancellationToken::is_cancelled()`，该信号不会强行终止线程或 IO。外部调用 token.cancel() 后，组件在下一次事件中处理取消并停止动画；静态帧 Loader 没有定时事件。

## 扩展功能示例

```powershell
cargo run -p bi-tui --example features
cargo run -p bi-tui --example features -- --main
cargo run -p bi-tui --example features -- --overlay
cargo run -p bi-tui --example features -- --headless
```

默认按环境标识推断终端能力。`--kitty` 或 `--iterm2` 可以显式选择图片协议，只有支持该协议的终端才能显示图片。`--headless` 使用 TestBackend 并将协议编码写入内存，不向真实终端发送控制序列。示例使用稳定竖线光标，会话关闭时恢复默认形状。

| API | 用法与行为 |
| --- | --- |
| `Markdown::new(text)` | CommonMark/GFM 文本与 syntect 代码高亮，样式和链接随字素折行保留 |
| `Image::new(id, bytes, size)` | 编码图片文件数据，指定单元格尺寸；无图片能力时显示替代文本 |
| `Overlay::new(placements)` | 第一子项为底层，其余浮层按顺序叠加；可居中、指定位置或隐藏 |
| `SearchView::new(height)` | 单子项文档搜索，Ctrl+F 输入，Enter/Shift+Enter 跳转，Escape 关闭 |
| `Flash::new(duration)` | 使用 `Runtime::message_sender(handle)` 投递 `bi.flash` UTF-8 消息，定时过期 |
| `ScrollView::with_scrollbar(true)` | 预留一列显示滚动条，点击/拖动定位；部分滚动的剩余量继续向祖先传播 |
| `with_entries([StackEntry, ...])` | HStack/VStack 弹性尺寸；basis、grow、shrink、min、max 与 visible_from_width |
| `with_align(StackAlign::Stretch)` | 交叉轴拉伸；与既有 Start/Center/End 对齐共用接口 |
| `SettingsList::with_submenu(provider)` | Enter 时动态创建 SelectItem 菜单，Enter 确认，Escape 返回设置列表 |
| `with_fuzzy(true)` | 命令补全、路径补全、SelectList 和 SettingsList 可选择模糊子序列匹配 |
| `Editor::with_auto_complete(true)` | 输入后请求候选，仅显示菜单，不直接替换单候选 |
| `Keybindings::bind(from, to)` | 用字符串按键重映射，调用 Runtime::set_keybindings 应用；disable 禁用按键 |

Input/Editor 的 Ctrl+U/K/W 与 Alt+D/Backspace 保存被删除文本，连续删除可累积；Ctrl+Y 恢复，Alt+Y 轮换最近的恢复文本。它是组件内部缓冲区，不使用系统剪贴板。连续 Text 插入在光标连续且间隔小于 500ms 时合并为一次撤销，移动、粘贴和其他编辑动作结束合并。

`CombinedAutocomplete` 可组合 `CommandAutocomplete` 与 `PathAutocomplete`。文件补全基于指定目录枚举文件，保留参数后缀，为空格路径添加引号；权限等 IO 错误会返回组件错误。自动候选也在事件线程枚举目录，较慢文件系统可能影响输入响应。

弹性布局使用 `with_entries()` 时优先于原有 with_widths/with_heights 主轴配置。basis=None 使用测量尺寸，grow/shrink 按权重迭代分配并尊重 min/max；约束无法在父尺寸内满足时通过裁剪处理。visible_from_width 根据父宽度决定是否省略子项，省略项不占间距。配置仍按子节点位置对应，增删子节点时需要考虑位置变化。

`TerminalSession::main_screen_dynamic_with_input(max_height, modes)` 让视口随文档高度变化，限制在最大行数和终端高度内；`print_scrollback(text)` 显式提交主屏幕历史。两者不自动把历史组件文档变成永久终端记录。`Runtime::set_focus_scope(Some(popup))` 启用模态范围，None 关闭并恢复此前焦点；当前只支持一个活动范围。

## 模块职责

- `protocol`：事件、尺寸、样式、身份、状态和宿主接口。
- `component`：可变实例树与不可变布局快照。
- `autocomplete`：补全候选协议、斜杠命令、文件路径与组合提供者。
- `layout`：快照校验、全局坐标转换、裁剪和命中测试。
- `render`：绘制到 Ratatui Buffer，计算可见焦点光标。
- `runtime`：挂载、卸载、替换、焦点、事件传播、消息和资源清理。
- `terminal`：主屏幕/备用屏幕会话、Crossterm 事件转换和统一事件循环。
- `widgets`：内置组件。
- `utils::text`：纯文本列宽、换行、前缀截取和截断。

`Runtime::append_child()` 会执行挂载及身份注册；`ComponentNode::add_child()` 仅修改实例树。需要消息、定时器或资源清理的组件应通过 Runtime 管理。

## 当前边界

颜色、加粗等 Style 已转换到 Ratatui，不需要组件输出 ANSI 字符串。硬件光标只显示在焦点组件的可见区域。主屏幕支持固定高度或随内容变化的 Inline 视口，以及显式提交滚动历史；仍不等同于 TS 的完整主屏幕文档渲染算法。

已接入 Markdown、代码高亮、路径补全、终端图片、OSC 8 链接与浮层。基础 LaTeX 已转为 Unicode，复杂展示数学排版及 TS 特定快捷键/渲染细节尚未完全对齐，具体差异见运行时说明。组件原始文本不接受 ANSI 控制序列。

## 检查

```powershell
cargo test -p bi-tui
cargo fmt -p bi-tui -- --check
cargo clippy -p bi-tui --all-targets -- -D warnings
```
