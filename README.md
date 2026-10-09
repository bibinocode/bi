# bi 

这是一个仿写的pi agent版本的rust实现，用于学习pi的设计和实现。

原项目[pi](https://github.com/earendil-works/pi)

## 项目目录

项目采用 Cargo workspace，五个核心模块位于 `crates/`，根目录 `src/main.rs` 保留为 `bi` 程序入口。目录结构与模块依赖见 [架构说明](docs/architecture.md)。TUI 已具备组件布局、绘制、运行时和基础交互组件，使用方法见 [TUI 说明](crates/tui/README.md)；其他核心模块仍为骨架。


## 核心模块

基础需要实现的5个核心模块：

1. ai: 只管调用模型，可以单独做为一个库向外暴露，解决统一 LLM API，支持自动模型发现和提供商配置
2. agent: 实现 Agnet 的核心循环，解决怎么让 LLM 反复思考和行动，通用 Agent 框架，支持传输抽象、状态管理和附件支持
3. coding-agent: 编程 Agent CLI，提供读、执行、编辑、写工具和会话管理，完整业务流程
4. tui: 终端 UI 库，负责在终端里渲染 Markdown、代码高亮、差分显示。它的依赖里没有任何 AI 相关的包
5. codemode: 工具编排，JavaScript沙箱


## 技术选项

- TUI的选型：Ratatui + Crossterm 先做好事件循环、组件和插件边界，主屏幕行为出现明确限制时，再单独实现渲染器
