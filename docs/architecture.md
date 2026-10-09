# 项目架构

项目采用 Cargo workspace，按 README 中的五个核心模块拆分为独立 crate。TUI 已建立组件、布局、绘制、终端和运行时链路；其余核心模块仍为骨架。

```text
bi/
├── Cargo.toml                 # workspace 配置与 bi 二进制包
├── src/
│   └── main.rs                # bi 程序入口
├── crates/
│   ├── ai/                    # bi-ai：统一 LLM API
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── agent/                 # bi-agent：通用 Agent 核心循环
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── coding-agent/          # bi-coding-agent：编程 Agent 业务流程
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── tui/                   # bi-tui：终端 UI 库
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── codemode/              # bi-codemode：工具编排与 JavaScript 沙箱
│       ├── Cargo.toml
│       └── src/lib.rs
└── docs/
    └── architecture.md        # 目录、职责与依赖边界
```

## 模块职责与依赖

下表列出当前 Cargo 清单声明的直接内部依赖。依赖关系用于建立模块边界。

| 包 | 职责 | 直接内部依赖 |
| --- | --- | --- |
| `bi` | CLI 程序入口，后续接入编程 Agent 业务流程 | `bi-coding-agent` |
| `bi-ai` | 统一 LLM API、自动模型发现、提供商配置，可单独作为库使用 | 无 |
| `bi-agent` | 通用 Agent 核心循环、传输抽象、状态管理、附件支持 | `bi-ai` |
| `bi-coding-agent` | 编程 Agent 业务流程、读/执行/编辑/写工具、会话管理 | `bi-ai`、`bi-agent`、`bi-tui`、`bi-codemode` |
| `bi-tui` | 组件布局、Ratatui 绘制、终端会话、事件循环、焦点与资源管理；Markdown 和代码高亮待实现 | 无 |
| `bi-codemode` | 工具编排与 JavaScript 沙箱 | 无 |

`bi-tui` 不依赖 AI 相关模块。已接入 Ratatui、Crossterm、thiserror、unicode-segmentation 和 unicode-width；组件使用结构化快照表达内容，不直接输出 ANSI。运行时设计与限制见 [TUI 运行时](tui-runtime.md)。

`bi-codemode` 先保持独立，JavaScript 沙箱实现和工具接口将在开发该模块时确定。

## 本地检查

在项目根目录执行以下命令，检查所有 workspace 成员是否能够编译：

```powershell
cargo check --workspace
```
