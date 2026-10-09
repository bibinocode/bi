# TUI 运行时与 TS 参考范围

本轮范围是现有架构上的运行时闭环和常用交互组件。参考代码位于 `E:\abi\rust\pi\packages\tui`，Rust 版保留项目现有的结构化协议和 Ratatui/Crossterm 方案，不直接移植 TS 的 ANSI 字符串渲染器。

## 运行链路

```text
Crossterm → TerminalEvent → Runtime 事件路由 → Component
                                             ↓
                           LayoutNode / LayoutSnapshot
                                             ↓
                 校验 → 坐标转换与裁剪 → Ratatui Buffer
                                             ↓
                           Ratatui 差异更新 → 终端
```

`terminal::run()` 每轮最多处理 `RunOptions.message_budget` 条消息，再按重绘请求布局和绘制，随后轮询终端事件。默认轮询间隔为 16 ms，消息预算为 64；二者必须大于零。没有重绘请求时不重新布局或绘制，消息和终端输入不会分别启动两套 UI 线程。

组件回调、布局和消息派发都在调用 `run()` 的线程执行。Loader 的后台线程仅发送消息；组件回调需要保持短时执行。消息队列目前没有容量上限，消息预算限制每轮消费量，不限制生产者入队量。

## 事件与焦点

按键先派发 `Key`。未消费且不是释放事件时，才派发该按键对应的 `Text`；粘贴作为一次完整 `Paste` 派发。快捷键不会同时插入文本。默认不向组件派发按键释放事件，组件可通过 `wants_key_release()` 选择接收。

键盘和粘贴从焦点组件向祖先传播。鼠标先命中最上层可见节点，再沿布局路径向祖先传播；各组件收到的是局部坐标。捕获指针后，即使鼠标移出视口，拖动和释放也会到达捕获组件。按下和释放位置、按钮一致且未拖动时，宿主生成 `Click`；同位置同按钮在 500 ms 内连续点击时递增 `click_count`。

未被消费的 Tab/Shift+Tab 按实例树前序遍历循环切换焦点，只访问 `focusable()` 返回 true 的组件。当前不按视口可见性过滤焦点目标；被裁剪的焦点组件仍可接收键盘事件，但其硬件光标隐藏。

默认情况下，未被消费的 Escape 退出。组件消费 Escape 时，默认退出不执行；`RunOptions.exit_on_escape = false` 可关闭该退出规则。按下或重复 Ctrl+C 始终退出，释放 Ctrl+C 不退出。

`dispatch_terminal_event()` 和 `draw_runtime()` 可用于自行组织事件循环或使用 TestBackend 测试。传给鼠标派发的布局、根偏移和视口必须来自同一帧。

## 生命周期与状态

`mount_root()` 和 `append_child()` 由运行时分配身份并挂载组件。挂载失败时，注册过的清理函数仍会执行。`remove()` 按子节点在前、父节点在后的顺序卸载子树，并清除子树内的焦点和指针捕获。

`replace()` 保留逻辑 ID 和子节点。它先保存旧组件状态，为候选分配新的 generation，再恢复状态、挂载、提交。恢复或挂载失败时保留旧实例；失败候选的 generation 不复用。提交后清理旧组件资源，旧组件卸载失败不会回滚新实例。因此 `replace()` 返回错误后，调用方可通过 `active_handle(current.id)` 检查当前有效实例：仍为旧句柄表示候选未提交，变为新句柄表示已经提交但后续清理或通知失败。

Input 状态版本为 1，保存文本和字素簇边界上的 UTF-8 字节光标；不保存撤销栈、样式、提示符或提交回调。恢复前校验版本、编码、文本和光标，非法状态不覆盖现有输入。

Editor 状态版本为 1，使用 `application/x-bi-editor` 编码，保存编辑文本、字节光标、粘贴 ID 与范围、粘贴原文；不保存撤销栈、历史、补全菜单、回调或 UI 尺寸。解码先检查长度、UTF-8、ID 唯一性、标记文本、范围和原子光标边界，再覆盖当前文档。粘贴 ID 在同一文档中递增，删除标记不会重编号；提交清空文档后重新从 1 开始。

消息实际派发前检查 source 和 target 的完整句柄。已经卸载或替换的实例发送的消息会被丢弃。注册清理函数按注册顺序执行；单个清理函数 panic 不会跳过其余资源。展开式组件 panic 转为 `ComponentError`，此机制不适用于 `panic = abort`。

`run()` 退出或失败后执行 `Runtime::shutdown()`。调用方随后应显式 `TerminalSession::close()` 以报告终端恢复错误；忘记显式关闭时由 Drop 尝试恢复。终端恢复只撤销本会话记录的输入模式和 raw mode 责任，不保证能恢复进程被强制终止时的状态。

## TS 参考与差异

| 参考模块 | Rust 对应实现 | 当前边界 |
| --- | --- | --- |
| `tui.ts`、`layout.ts`、`layout-node.ts` | component / protocol / layout / runtime | 保留 Rust 快照和实例版本模型；未实现 overlay |
| `terminal.ts`、`tui-main-screen.ts`、`tui-alt-screen.ts` | TerminalSession、run、draw_runtime | 主屏幕为 Ratatui 固定高度 Inline 视口，不等同于 TS 动态文档滚动；未实现 TS 同步输出控制序列 |
| `components/input.ts` | Input | 单行输入、字素编辑、水平滚动、提交、粘贴、单词导航、100 步撤销、状态恢复；没有 kill ring、yank 或撤销合并 |
| `components/truncated-text.ts` | TruncatedText | 第一行截断和 Unicode 列宽；内边距由 BoxContainer 提供 |
| `components/scroll-view.ts` | ScrollView | 一个子组件、纵向偏移、固定期望高度、可选跟随末尾、滚轮和键盘；边界可冒泡，未传递部分滚动的剩余量，未实现滚动条 |
| `components/select-list.ts` | SelectList | 不区分大小写的 value 前缀过滤、循环导航、滚轮、点击和回调；描述与标签同一行，不使用 TS 的自适应描述列 |
| `components/loader.ts`、`components/cancellable-loader.ts` | Loader、CancellableLoader、CancellationToken | 消息驱动帧切换，挂载创建定时线程、卸载取消；Escape 停止动画并设置协作取消信号，回调仅执行一次；不对应 JS AbortSignal 的完整 API |
| `components/editor.ts`、`editor-component.ts` | Editor | 多行字素编辑、硬换行、可见行滚动、撤销、历史、原子粘贴标记和状态恢复；Enter 提交后清空，Alt/Shift/Ctrl+Enter 换行；Alt+Up/Down 浏览历史，与 TS 普通上下键历史规则不同；没有 kill ring、撤销合并、主题边框或大单行字符数压缩 |
| `autocomplete.ts` | AutocompleteProvider、CommandAutocomplete | 按 Tab 请求斜杠命令候选，匹配当前逻辑行开头的命令前缀；没有路径补全、模糊排序或输入时自动弹出候选 |
| `components/settings-list.ts` | SettingsList、SettingItem | 值循环、只读项、标签子串搜索、描述、鼠标与回调；上下导航不循环，没有 TS 模糊搜索、动态子菜单或列对齐；描述最多三行 |
| `utils.ts` | utils::text | 纯文本列宽、硬换行和截断，拒绝控制字符；不解析 ANSI |

Markdown、文件补全、图片协议、OSC 8 超链接输出和横向 Stack 尚未实现。可见布局中的图片请求会明确报错，超链接元数据目前不转换成终端控制序列。

## 验证

```powershell
cargo test -p bi-tui
cargo clippy -p bi-tui --all-targets -- -D warnings
cargo run -p bi-tui --example headless
cargo run -p bi-tui --example editor -- --headless
```

自动化覆盖输入字素编辑、状态恢复、队列版本校验、候选失败回退、清理失败后继续卸载、焦点、点击与捕获、嵌套裁剪、光标、Unicode 绘制、旧帧清除、Loader 消息、多行编辑、补全范围校验、原子粘贴、编辑历史、设置搜索与取消。真实终端中的键盘协议、鼠标支持、字体宽度与 raw mode 恢复仍需要在目标终端中运行交互示例检查。
