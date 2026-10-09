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

未被消费的 Tab/Shift+Tab 按实例树前序遍历循环切换焦点，只访问 `focusable()` 返回 true 的组件。下一次布局后不访问因响应式规则或浮层配置而省略的节点，但不按视口裁剪过滤焦点目标；被裁剪的焦点组件仍可接收键盘事件，但其硬件光标隐藏。

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

| 参考模块 | Rust 对应实现 | 行为与差异 |
| --- | --- | --- |
| `tui.ts`、`layout.ts`、`layout-node.ts` | component / protocol / layout / runtime、Overlay | 保留结构化快照、实例版本、资源与消息模型；浮层按绘制顺序合成，支持居中/指定位置/隐藏；模态焦点限制为一个活动子树 |
| `terminal.ts`、`tui-main-screen.ts`、`tui-alt-screen.ts` | TerminalSession、run、draw_runtime、TerminalOutput | 固定或动态高度 Inline 视口；显式 print_scrollback 提交历史；同步更新包裹文本与外部协议；不自动把组件文档变化提交为历史，也不完全复刻 TS 主屏幕文档算法 |
| `components/input.ts`、`components/editor.ts`、`kill-ring.ts`、`undo-stack.ts` | Input、Editor、KillRing | 字素编辑、单词导航、kill/yank/yank-pop；连续 Text 插入在 500ms 内合并撤销；最多 100 撤销与删除记录；Editor 原子粘贴、状态恢复、历史；Alt+Up/Down 浏览历史，仍与 TS 普通上下键规则不同 |
| `autocomplete.ts`、`fuzzy.ts` | CommandAutocomplete、PathAutocomplete、CombinedAutocomplete、fuzzy_score | 斜杠命令、文件/目录、带空格引号、Unicode、可选子序列排序；编辑器可输入时请求候选；同步枚举目录，无外部搜索进程，评分算法不保证与 TS 相同 |
| `components/markdown.ts` | Markdown、styled_text、pulldown-cmark、syntect | 标题、强调、删除线、任务列表、引用、表格文本、代码块高亮和链接元数据；结构化字素折行；HTML 显示为文本，Markdown 图片显示替代文本；基础数学表达式转为 Unicode（希腊字母、上下标、根号、行内分式等），未知/不完整语法保留源码；没有 TS 的多行矩阵和展示分式排版 |
| `components/image.ts`、`terminal-image.ts` | Image、TerminalOutput | PNG/JPEG/GIF/WebP 解码后转 PNG，Kitty/iTerm2 编码，裁剪及浮层遮挡；无能力时替代文本；GIF 输出静态解码帧，不播放动画 |
| `components/scroll-view.ts` | ScrollView | 可选滚动条及点击定位；滚动边界向祖先传递剩余 rows；单子项、跟随末尾、键盘/滚轮；支持指针捕获后的滚动条拖动 |
| `alt-screen-search.ts` | SearchView | Ctrl+F、Enter/Shift+Enter 跳转、Escape 关闭；区分大小写的单行匹配，匹配行去重；不跨行，不保证 TS 搜索所有快捷键与提示位置导航一致 |
| `components/alt-screen-flash.ts` | Flash | bi.flash 消息添加提示，定时消息过期，卸载取消线程；可作为普通组件放置，不限定备用屏幕 |
| `components/select-list.ts` | SelectList | 可选模糊排序、循环导航、自适应标签/描述列；前缀过滤保持默认行为 |
| `components/settings-list.ts` | SettingsList、SettingItem | 值循环、只读项、标签搜索、可选模糊排序、列对齐、Enter 动态子菜单与修改回调；描述最多三行，点击仍执行普通值循环 |
| `components/h-stack.ts`、`components/v-stack.ts`、`components/stack.ts` | HStack、VStack、StackEntry | 保留简易分配 API；with_entries 支持 basis/grow/shrink/min/max、stretch、按父宽度隐藏；隐藏项不布局也不占间距；配置按子节点位置对应 |
| `components/mouse-region.ts` | MouseRegion | 单子项、事件冒泡、局部坐标、捕获响应；不生成进入/离开事件 |
| `components/loader.ts`、`components/cancellable-loader.ts` | Loader、CancellableLoader、CancellationToken | 消息驱动动画与协作取消；不对应 JS AbortSignal 完整 API |
| `keys.ts`、`keybindings.ts` | KeyChord、Keybindings | 字符串解析、布局键匹配、会话级重映射与禁用；目标为规范按键，不照搬 TS 字符串动作名注册表；Ctrl+C 的宿主退出规则优先于重映射 |
| `utils.ts`、颜色与原生输入辅助 | utils / protocol / Crossterm / Ratatui | 列宽、文本折行、截断、结构化样式；不解析用户提供的 ANSI，不移植 TS 的 native 模块加载与 ANSI 字符串渲染器 |

`draw_runtime()` 支持 TestBackend，也生成最终 Buffer 供外部协议输出。它本身不会向 stdout 发送图片或链接；`run()` 调用 `TerminalOutput::write_frame()` 完成这一步。裸 `paint_layout()` 仍拒绝图片，调用 `paint_layout_with_images()` 才允许预留图片区域。图片经过裁剪与上层文本/背景遮挡后按可见矩形编码；Kitty 只删除本输出器生成的 ID，iTerm2 在下一帧清除并重画文本视口。图片编码在 UI 线程执行，大图可能阻塞输入，不把它描述成后台异步渲染。

`RunOptions::default()` 通过环境标识推断能力，调用方可以覆盖 capabilities；推断不保证实际终端支持，也不发送主动探测查询。没有图片能力时 Image 使用替代文本。超链接 URL 拒绝控制字符；内容始终是结构化纯文本，终端控制序列仅由输出器生成。链接目标变化或移除时重新写对应单元格，避免旧目标残留。

`Runtime::set_focus_scope(Some(handle))` 限制焦点、Tab 与事件传播到子树，None 关闭并恢复此前仍有效的焦点。隐藏浮层前应关闭它的焦点范围；动态隐藏的节点在下一次布局后不参与 Tab。普通被裁剪节点仍保留原有焦点遍历行为。`message_sender(handle)` 提供带实例身份的发送端，旧实例消息仍按现有版本规则丢弃。

HStack 的宽度配置按子节点位置对应，未配置项按 Fill(1) 处理；Fixed 项按顺序优先分配，Fill(0) 不获得宽度。间距先从宽度预算中扣除，超出视口的间距被裁剪。零宽子项仍执行布局以更新组件缓存，但不撑高容器。最大余数法分配按权重计算后的整数剩余列，余数相同时优先靠前的子项。被压成零宽的可聚焦节点仍参与现有焦点遍历，光标因不可见而隐藏。

VStack 默认 Auto 高度，先测量各子项完整内容。有父高度上限时先扣除间距，按顺序分配 Auto/Fixed，再通过最大余数法让 Fill 分享剩余行；之后按分配高度重新布局子项，使编辑器、滚动视图和嵌套 Stack 获得实际高度上限。Fixed 的空白行也计入容器高度，各子项有独立裁剪范围。没有高度上限时 Auto 与正权重 Fill 使用完整内容高度。与 Container 的完整快照裁剪方式不同，受限的 VStack 子项会按分配高度重新生成快照。

## 验证

```powershell
cargo test -p bi-tui
cargo clippy -p bi-tui --all-targets -- -D warnings
cargo run -p bi-tui --example headless
cargo run -p bi-tui --example editor -- --headless
cargo run -p bi-tui --example stack -- --headless
cargo run -p bi-tui --example features -- --headless
```

自动化覆盖输入字素编辑、状态恢复、队列版本校验、候选失败回退、清理失败后继续卸载、焦点、点击与捕获、嵌套裁剪、光标、Unicode 绘制、旧帧清除、Loader 消息、多行编辑、补全范围校验、原子粘贴、编辑历史、设置搜索与取消。真实终端中的键盘协议、鼠标支持、字体宽度与 raw mode 恢复仍需要在目标终端中运行交互示例检查。

新增自动化覆盖 Markdown/高亮、结构化样式折行、路径与引号、模糊匹配、自动候选、kill/yank/撤销合并、弹性尺寸、隐藏焦点、模态恢复、搜索、动态设置子菜单、临时提示、剩余滚动量、图片裁剪/遮挡与协议编码、链接移除及快捷键重映射。动态主屏幕、同步更新和图片的真实显示仍需目标终端验证。
