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

## 已有组件

| 组件 | 功能 |
| --- | --- |
| `Container` | 子组件纵向排列，保留完整子快照，按容器边界裁剪 |
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

SettingsList 的 Enter 或空查询时的 Space 循环切换 `values`，空 `values` 为只读项。`with_search(true)` 启用标签的不区分大小写子串搜索，Backspace 删除查询中的一个字素簇，Escape 先清除非空查询，再执行取消回调或交给宿主退出。通过 `update_value()` 修改设置不会触发用户修改回调；目前没有子菜单。

CancellableLoader 需要获得焦点才能响应 Escape。第一次 Escape 设置取消信号、停止并清理动画定时线程，`on_abort` 仅执行一次；第二次 Escape 留给宿主退出。卸载也会设置取消信号，但不会调用 `on_abort`。后台任务必须自行检查 `CancellationToken::is_cancelled()`，该信号不会强行终止线程或 IO。外部调用 token.cancel() 后，组件在下一次事件中处理取消并停止动画；静态帧 Loader 没有定时事件。

## 模块职责

- `protocol`：事件、尺寸、样式、身份、状态和宿主接口。
- `component`：可变实例树与不可变布局快照。
- `autocomplete`：补全候选协议与斜杠命令提供者。
- `layout`：快照校验、全局坐标转换、裁剪和命中测试。
- `render`：绘制到 Ratatui Buffer，计算可见焦点光标。
- `runtime`：挂载、卸载、替换、焦点、事件传播、消息和资源清理。
- `terminal`：主屏幕/备用屏幕会话、Crossterm 事件转换和统一事件循环。
- `widgets`：内置组件。
- `utils::text`：纯文本列宽、换行、前缀截取和截断。

`Runtime::append_child()` 会执行挂载及身份注册；`ComponentNode::add_child()` 仅修改实例树。需要消息、定时器或资源清理的组件应通过 Runtime 管理。

## 当前边界

颜色、加粗等 Style 已转换到 Ratatui，不需要组件输出 ANSI 字符串。硬件光标只显示在焦点组件的可见区域。主屏幕目前是固定高度 Inline 视口，内容滚动由 ScrollView 管理，不等同于 TS 的动态主屏幕文档渲染。

Markdown、文件路径自动补全、图片输出、超链接控制序列和 overlay 尚未实现；Editor 和 SettingsList 的当前功能也不等同于 TS 完整实现，具体差异见运行时说明。组件原始文本不接受 ANSI 控制序列。

## 检查

```powershell
cargo test -p bi-tui
cargo fmt -p bi-tui -- --check
cargo clippy -p bi-tui --all-targets -- -D warnings
```
