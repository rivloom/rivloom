# Rivloom Desktop 架构设计

- 状态：历史基线；A0–A2 实现仍有效，chat-first 产品顺序已被取代
- 日期：2026-08-24
- 首发平台：Windows
- 产品形态：公开开源的商业桌面产品

> 路线更新（2026-08-30）：当前权威设计为
> [Runtime Host 与协作闭环设计](2026-08-30-runtime-host-collaboration-design.md)。本文保留
> Tauri 外壳、App Server sidecar、独立 `CODEX_HOME`、本地项目、安全和上游同步等已经
> 验证的工程决策；第 1、2、7、13、14 节中的单机 Chat 产品范围和交付顺序不再指导近期
> 开发。近期第一目标改为两人、两 Node 的 Codex 任务委派与 Patch 审查闭环。

## 1. 目标

Rivloom Desktop 第一阶段提供一个可独立安装的本地 AI 编程客户端。用户无需预装
Rust、Node.js 或 Codex CLI，即可完成以下闭环：

1. 使用 ChatGPT/Codex 账号登录。
2. 选择本地项目目录。
3. 创建或恢复对话。
4. 向 AI 提交编程任务并接收流式结果。
5. 审批命令和文件修改。
6. 查看代码差异与执行结果。

网络代理是第二阶段能力；多人协作和云端同步在单机版稳定后建设。

## 2. 范围

### 2.1 第一版包含

- App Server 启动、监控、关闭与一次自动恢复。
- ChatGPT/Codex 托管登录、账号状态和退出登录。
- 本地项目选择与最近项目列表。
- 对话列表、流式聊天和工具执行状态。
- 命令审批、文件修改审批和代码 Diff。
- 基础设置、诊断信息和可理解的错误提示。
- Rivloom 独立的数据、日志、缓存和配置目录。

### 2.2 第一版不包含

- 多人实时协作、组织和团队管理。
- 云端对话同步。
- 插件市场。
- 自建模型账号体系。
- 对官方 Codex Desktop 数据目录的实时共用。

## 3. 技术路线

采用 **Tauri + React + 独立 Codex App Server 子进程**：

```text
┌────────────────────────────────────────────┐
│ React / TypeScript                         │
│ 页面、状态展示、交互和本地化               │
└──────────────────────┬─────────────────────┘
                       │ Tauri IPC
┌──────────────────────▼─────────────────────┐
│ Tauri / Rust                               │
│ 进程监管、文件权限、协议桥接、日志和更新   │
└──────────────────────┬─────────────────────┘
                       │ stdio + JSONL
┌──────────────────────▼─────────────────────┐
│ codex-app-server                           │
│ 登录、对话、模型、工具、审批、沙箱和配置   │
└──────────────────────┬─────────────────────┘
                       │ HTTPS
┌──────────────────────▼─────────────────────┐
│ ChatGPT/Codex 服务                         │
└────────────────────────────────────────────┘
```

App Server 作为 Tauri sidecar 随安装包发布。桌面端不把 App Server 暴露为本地或公网
HTTP 服务，也不把其标准输入输出直接交给 React。

### 3.1 为什么选择 Tauri

- 与 Codex 的 Rust 技术栈一致。
- 桌面外壳和通常的运行内存小于自带 Chromium 的 Electron。
- React 默认不能直接访问本机敏感能力，权限边界更容易收敛。
- Tauri 后端适合监管 App Server 和现有 Rust 网络代理组件。

代价是团队需要同时维护 React 和少量 Rust，并分别测试 Windows、macOS 和 Linux
使用的 WebView。首发只承诺 Windows，代码设计保持跨平台。

### 3.2 未采用的方案

**Electron + React**：前端团队上手更快，跨平台渲染更一致，但需要额外携带 Chromium
和 Node.js，安装包、内存占用与运行时攻击面通常更大。

**纯 Rust UI 或直接嵌入 Codex crate**：会把桌面生命周期与 Codex 内部 API 紧密
绑定，增加 UI 开发成本，也让上游更新更难，不适合第一版。

## 4. 组件职责

### 4.1 React 界面

- 显示启动、登录、首页、工作区、审批、Diff 和设置页面。
- 将用户意图转换为受控的 Tauri 命令。
- 消费经过 Tauri 归一化的 App Server 事件。
- 保存尚未发送的输入草稿，但不保存访问令牌。

React 不直接启动进程、不直接读取任意文件、不直接访问 App Server 的 stdin/stdout。

### 4.2 Tauri 后端

- 解析 Rivloom 的应用数据目录。
- 为子进程设置独立的 `CODEX_HOME`。
- 启动、监控和正常关闭 `codex-app-server`。
- 维护请求 ID 与前端 Promise 的映射。
- 解析 stdout 上的 JSONL 协议消息，将 stderr 写入脱敏日志。
- 向 React 转发响应、通知、审批请求和连接状态。
- 管理文件选择器、项目目录授权、应用更新和签名验证。
- 在 App Server 意外退出后自动重启一次。

### 4.3 Codex App Server

- 管理 ChatGPT/Codex OAuth 登录和令牌刷新。
- 管理线程、轮次、流式条目和历史记录。
- 调用模型、工具、沙箱、MCP、技能和模型提供方。
- 发起需要用户处理的审批请求。

第一版优先使用其现有公开协议，不为 Rivloom 修改 `codex-core`。只有公开协议确实缺少
必要能力时，才单独评审 App Server v2 的扩展。

## 5. 生命周期与通信

### 5.1 启动

1. Tauri 创建或验证 Rivloom 应用目录。
2. Tauri 定位与当前桌面版本配套的 App Server sidecar。
3. Tauri 设置独立 `CODEX_HOME` 并启动子进程。
4. Tauri 通过 stdio 发送 `initialize`。
5. 初始化成功后通知 React，界面进入登录页或首页。
6. 初始化超时或协议不兼容时阻止进入工作区，并显示诊断信息。

### 5.2 正常通信

```text
React command
  -> Tauri validates and assigns a local request ID
  -> App Server JSONL request
  -> zero or more streaming notifications
  -> final response or error
  -> Tauri normalizes the result
  -> React state update
```

Tauri 必须支持 App Server 主动发起的审批请求，不能把协议假设成简单的“一问一答”。

### 5.3 关闭与恢复

- 正常退出时先停止接收新请求，再正常关闭 App Server。
- App Server 意外退出时，所有未完成请求以明确错误结束。
- React 保留未发送草稿，显示“核心服务已断开”。
- Tauri 自动重启一次；连续失败后停止重试并提供手动重试。
- 恢复后通过 App Server 历史接口重新加载已持久化对话，不伪造中断请求的成功状态。

## 6. 登录与数据隔离

Rivloom 不共享官方 Codex 的 `.codex` 目录。Tauri 为 App Server 设置 Rivloom 专属的
`CODEX_HOME`。Windows 的实际根目录由 Tauri 的本地应用数据目录 API 解析，不在代码中
硬编码用户名或磁盘路径。

逻辑目录如下：

```text
Rivloom/
├─ codex-home/    App Server 登录状态、对话和 Codex 配置
├─ settings/      Rivloom 界面与产品设置
├─ logs/          脱敏运行日志
└─ cache/         可安全清理的临时数据
```

登录只采用 App Server 管理的 ChatGPT 浏览器 OAuth 流程。设备码能力保留在上游 App
Server，但不作为 Rivloom Desktop 首发产品入口。React 只能收到登录状态和必要的公开
账号信息，不能读取或记录 OAuth 令牌。

未来若提供“从 Codex 导入”，必须是用户主动发起、可预览、可取消的一次性迁移，不能
让两个产品长期同时读写同一数据目录。

## 7. 页面与用户流程

### 7.1 启动页

- 展示 App Server 的查找、启动、初始化和恢复状态。
- 失败时提供简明原因、重试入口和日志位置。

### 7.2 登录页

- 发起浏览器 OAuth。
- 展示等待、成功、失败和取消状态。
- 浏览器回调不可用时允许取消并重试。

### 7.3 首页

- 打开本地项目。
- 展示最近项目。
- 显示账号和核心服务状态。

### 7.4 工作区

- 对话列表与当前对话。
- 消息输入和流式回答。
- 工具执行进度与结果。
- 审批卡片与代码 Diff。

### 7.5 设置与诊断

- 账号、数据位置、日志和版本信息。
- 第二阶段加入代理配置、连通性测试和代理状态。

## 8. 网络代理的第二阶段设计边界

- 默认关闭，必须由用户主动启用。
- 支持明确配置的 HTTP 和 SOCKS 代理。
- 优先复用仓库现有 `codex-network-proxy`，不在 React 中重新实现代理引擎。
- 代理配置由 Tauri 校验和管理，敏感认证信息不写入普通日志。
- 提供独立的连通性测试和清晰的失败原因。
- 连接失败时不静默绕过用户配置，也不自动暴露本地代理端口到公网。
- HTTPS 中间人能力不作为普通用户默认功能启用。

## 9. 仓库和上游同步策略

```text
rivloom/
├─ apps/
│  └─ desktop/        Rivloom Tauri + React 应用
├─ codex-rs/          上游 Codex 核心
├─ rivloom-docs/      Rivloom 设计、计划与 ADR
├─ LICENSE
└─ NOTICE
```

- Rivloom 新功能优先进入 `apps/desktop` 或独立的 Rivloom crate。
- 避免全局替换 Codex/OpenAI 标识，避免无必要修改 `codex-rs`。
- 产品开发开始后，`main` 保存 Rivloom 产品状态。
- 定期从 `upstream/main` 创建更新分支，合并、构建和测试后通过 PR 进入 `main`。
- 不对已公开的产品主分支使用破坏历史的强制推送来维持线性历史。
- 如果必须修改上游文件，在变更记录中说明 Rivloom 的修改，并保持提交范围小而清晰。

## 10. 安全与合规

- React 不持有 OAuth 令牌或不受控的本机执行能力。
- App Server 仅通过子进程 stdio 暴露给 Tauri。
- 仅把用户明确选择的项目目录交给 Codex。
- 延续 Codex 的沙箱和审批机制，不绕过敏感操作确认。
- 日志对令牌、账号信息和敏感环境变量进行脱敏。
- 更新包必须验证签名；验证失败不得覆盖已安装版本。
- 发布物保留适用的 Apache 2.0 `LICENSE`、`NOTICE`、版权与修改说明。
- Rivloom 使用独立品牌，不能暗示是 OpenAI 官方客户端。
- 正式发布生成第三方依赖清单和 SBOM，并建立 Windows 代码签名流程。

本节是工程合规要求，不替代针对具体发行版本的法律审查。

## 11. 非功能要求

### 11.1 兼容性

- 第一版支持 Windows 10/11 x64。
- 用户电脑不需要预装 Rust、Node.js 或 Codex CLI。
- 架构不依赖 Windows 专属协议，以便后续支持 macOS 和 Linux。

### 11.2 性能与体验

- App Server 正常时，桌面启动过程应持续显示状态，不出现无反馈的空白窗口。
- 流式输出和工具事件不得阻塞界面交互。
- 长对话和大量事件必须采用有界缓存或虚拟列表，不能无限占用前端内存。

### 11.3 可靠性

- 未发送的输入草稿在 App Server 崩溃时不丢失。
- 已持久化的对话在重启桌面应用后可恢复。
- 协议不兼容、登录失败、无目录权限和网络中断均有明确的失败状态。
- 日志写入失败不能导致主工作区崩溃。

### 11.4 可维护性

- 桌面 UI、进程桥接和 App Server 协议适配分层维护。
- App Server 版本与桌面版本在构建时绑定，并在初始化时检查兼容性。
- CI 至少覆盖 React 检查、Rust 检查、协议桥接测试和 Windows 打包冒烟测试。

## 12. 主要故障模式

| 故障                  | 用户影响         | 处理策略                               |
| --------------------- | ---------------- | -------------------------------------- |
| sidecar 缺失或损坏    | 无法进入工作区   | 阻止启动并建议重新安装                 |
| App Server 初始化超时 | 启动停滞         | 超时、终止子进程、显示重试和日志       |
| App Server 运行中崩溃 | 当前请求中断     | 保存草稿、失败未完成请求、自动重启一次 |
| 协议版本不匹配        | 消息无法正确解释 | 启动时检查并阻止不安全降级             |
| OAuth 浏览器回调失败  | 无法完成登录     | 允许取消并重试浏览器登录               |
| 项目目录无权限        | 无法读写项目     | 不执行操作并要求重新选择目录           |
| 网络中断              | 回答中断         | 保留对话视图并允许恢复后重试           |
| 代理不可达            | 模型请求失败     | 显示代理诊断，不静默绕过配置           |
| 日志包含敏感信息      | 凭证泄露         | 结构化脱敏、限制字段、发布前安全测试   |

## 13. 交付里程碑

> 本节是 2026-08-24 的历史计划。当前里程碑以
> [Runtime Host 与协作闭环设计第 14 节](2026-08-30-runtime-host-collaboration-design.md#14-里程碑-gate)
> 为准；详细迁移计划将单独发布和审查。

1. **桌面空壳**：窗口、品牌、导航、sidecar 启动和连接状态。
2. **账号登录**：独立数据目录、OAuth、账号状态和退出登录。
3. **本地项目**：目录选择、最近项目、创建和恢复对话。
4. **编程闭环**：流式回答、工具状态、审批和 Diff。
5. **稳定性与代理**：崩溃恢复、诊断、HTTP/SOCKS 设置与测试。
6. **公开发行**：Windows 安装包、签名、合规清单、SBOM 和 GitHub 发布流程。

## 14. 第一版验收标准

> 本节记录原单机客户端验收。当前第一版验收已改为两人、两 Node 的协作闭环，详见
> [当前权威设计第 15 节](2026-08-30-runtime-host-collaboration-design.md#15-第一版验收标准)。

在一台没有安装 Rust、Node.js 和 Codex CLI 的受支持 Windows 电脑上，用户可以：

1. 安装并启动 Rivloom。
2. 使用 ChatGPT/Codex 账号完成登录。
3. 打开本地项目并创建编程对话。
4. 接收流式回答，查看工具状态并处理审批。
5. 查看 AI 造成的代码 Diff。
6. 关闭并重新打开 Rivloom 后恢复已持久化对话。
7. 确认官方 Codex 的数据和配置没有被 Rivloom 修改。

## 15. 已接受的架构决策

- [ADR-0001：采用 Tauri、React 与 App Server sidecar](../adr/0001-use-tauri-react-and-app-server-sidecar.md)
- [ADR-0002：隔离 Rivloom 的 Codex 数据目录](../adr/0002-isolate-rivloom-codex-home.md)
- [ADR-0003：分离 Rivloom 产品代码并以合并方式同步上游](../adr/0003-separate-rivloom-code-from-upstream-codex.md)
- [ADR-0004：以稳定 cwd 协议表示本地项目](../adr/0004-use-stable-cwd-for-local-projects.md)
- [ADR-0005：采用外部 Agent Runtime](../adr/0005-use-external-agent-runtimes.md)
- [ADR-0006：分离 Rivloom Identity 与 Runtime Auth](../adr/0006-separate-rivloom-identity-from-runtime-auth.md)

## 16. 参考资料

- [Codex App Server documentation](https://learn.chatgpt.com/docs/app-server)
- [Tauri documentation](https://v2.tauri.app/)
- Repository `LICENSE` and `NOTICE`
- Repository root `AGENTS.md`
