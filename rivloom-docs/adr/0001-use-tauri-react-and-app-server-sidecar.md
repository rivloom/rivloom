# ADR-0001：采用 Tauri、React 与 App Server sidecar

## 状态

Accepted

## 背景

Rivloom 需要为开源 Codex 构建可公开发行的桌面界面。第一版以 Windows 为首发平台，
以后需要支持 macOS 和 Linux。桌面端必须管理登录、项目选择、流式对话、审批、Diff
和本地进程，同时尽量减少对上游 Codex 核心的改动。

## 决策

使用 React/TypeScript 构建界面，使用 Tauri/Rust 提供桌面能力。将
`codex-app-server` 编译为与桌面版本配套的 sidecar，由 Tauri 启动并通过 stdio JSONL
协议通信。

在 2026-08-30 的 Runtime Host 架构中，该 sidecar 被明确定位为第一个外部 Agent
Runtime，而不是 Rivloom 的源码内核。Rivloom 不依赖 `codex-core`；未来 Runtime 也通过
独立进程边界接入。详见
[ADR-0005](0005-use-external-agent-runtimes.md)。

React 只能通过受控 Tauri IPC 使用本机能力。App Server 不对本地网络或公网开放服务
端口。

## 结果

### 正面

- 与 Codex 的 Rust 技术栈一致。
- 桌面后端适合管理 App Server 和 Rust 网络代理组件。
- React 与本机敏感能力之间存在明确权限边界。
- App Server 保持独立，便于跟随上游构建和替换。
- 相比自带 Chromium 的方案，桌面外壳通常更小、内存占用更低。

### 负面

- 团队需要同时维护 React 和 Rust。
- 各平台 WebView 存在细微差异，需要分别测试。
- App Server sidecar 增加了版本配套、崩溃恢复和打包工作。

### 中性

- 最终安装包大小仍会受到 App Server 本身影响。
- 首发只支持 Windows，但接口和目录解析应保持跨平台。

## 考虑过的替代方案

**Electron + React**

- 优点：Node.js 生态成熟、前端团队上手快、跨平台渲染一致。
- 未采用：需要随应用携带 Chromium 和 Node.js，资源开销与运行时攻击面通常更大，
  且与现有 Rust 组件之间多一层技术栈。

**纯 Rust UI 或直接嵌入 Codex crate**

- 优点：单一原生语言和进程。
- 未采用：UI 开发成本高，并与 Codex 内部 API 紧密耦合，增加上游同步风险。

## 参考资料

- [Rivloom Desktop 架构设计](../plans/2026-08-24-rivloom-desktop-architecture-design.md)
- [Runtime Host 与协作闭环设计](../plans/2026-08-30-runtime-host-collaboration-design.md)
- [ADR-0005：采用外部 Agent Runtime](0005-use-external-agent-runtimes.md)
- [Codex App Server documentation](https://learn.chatgpt.com/docs/app-server)
- [Tauri documentation](https://v2.tauri.app/)
