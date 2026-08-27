# ADR-0004：以稳定 cwd 协议表示本地项目

## 状态

Proposed

## 背景

Rivloom A2 需要让用户选择本地目录、维护最近项目，并按项目创建、列出、读取和恢复
Codex thread。App Server v2 同时提供两组相关能力：稳定的 thread `cwd` 字段和筛选，
以及实验性的 `project/*`、thread `projectId`。

启用 `project/*` 要求整条 App Server 连接声明 `capabilities.experimentalApi=true`。这会
扩大 Rivloom 需要兼容和测试的实验协议面，而第一版本地项目只有一个目录根，不需要
多根项目、服务端排序或项目元数据同步。

## 决策

Rivloom 在自己的本地应用数据目录中维护一个有界的最近项目列表，以经后端验证和
规范化的绝对目录作为项目身份。App Server 继续只管理 thread：

- `thread/start` 通过稳定的 `cwd` 创建项目会话。
- `thread/list` 通过稳定的 `cwd` 精确筛选项目会话。
- `thread/read` 和 `thread/resume` 使用稳定的 thread ID，并核对返回的 `cwd`。
- 初始化不启用 `experimentalApi`，请求中不发送 `projectId`。

目录选择由 Tauri 官方对话框插件发起，所有路径在 Rust 后端重新验证；React 不获得
任意文件读写能力。选择项目或浏览历史不会调用 `turn/start`，只有用户明确创建会话时
才调用 `thread/start`，仍不会触发模型请求。

## 结果

### 正面

- 只依赖稳定 App Server v2 协议，降低上游升级和版本配套风险。
- 最近项目可以提供 Rivloom 自己需要的排序、失效目录和移除体验。
- 项目身份与 App Server 已持久化的 thread `cwd` 直接对应，不需要双重映射。
- 保持 Rivloom 与官方 Codex 数据目录隔离，不修改 `codex-rs`。

### 负面

- 最近项目元数据由 Rivloom 单独持久化，需处理损坏、迁移和原子写入。
- 第一版不支持一个项目包含多个目录根。
- 目录移动后旧 thread 不会自动归入新路径，需要用户重新选择或未来显式迁移。

### 中性

- App Server 的实验项目 API 成熟后，可以通过迁移层导入本地最近项目，不影响已有
  thread ID。
- A3 流式通知可能需要多观察者分发，但 A2 的请求—响应操作不提前建设该机制。

## 考虑过的替代方案

**直接采用实验 `project/*` 和 `projectId`**

- 优点：项目实体、排序和 thread 归属由 App Server 统一管理。
- 未采用：需要连接级实验能力，协议和存储语义仍可能变化，超出第一版需求。

**完全从 `thread/list` 推导项目列表**

- 优点：Rivloom 不需要单独持久化。
- 未采用：没有 thread 的新目录不会出现，失效目录和用户排序也无法可靠表达。

## 参考资料

- [Rivloom Desktop 架构设计](../plans/2026-08-24-rivloom-desktop-architecture-design.md)
- [A2 本地项目与会话设计草案](../plans/2026-08-27-desktop-local-projects-and-threads-design.md)
- `codex-rs/app-server/README.md`
- `codex-rs/app-server-protocol/src/protocol/v2/thread.rs`
- `codex-rs/app-server-protocol/src/protocol/v2/project.rs`
