# Rivloom Runtime Host 与协作闭环设计

- 状态：已确认，当前权威设计
- 日期：2026-08-30
- 首发平台：Windows
- 首个 Runtime：Codex App Server
- 第一验收目标：两个人、两台 Node 完成一次可审查的编程任务委派

## 1. 决策摘要

Rivloom 不再以“重新实现一个 Codex 桌面客户端”为主线，也不把 Codex 开源仓库中的
crate 嵌入为自己的 Agent 内核。Rivloom 要拥有的是协作控制面：用户与设备身份、Node、
任务、委派、权限、运行状态、Artifact 和审查；真正的 Agent loop 继续由外部 Runtime
执行。

第一版继续由 Tauri 拉起配套的 `codex-app-server` sidecar，通过稳定的 App Server v2
协议调用 Codex。当前 ChatGPT 登录保留，但其含义改为“本机 Codex Runtime 的认证”，
不能再充当 Rivloom 用户、团队或设备身份。

近期路线只支持 Codex，不提前实现通用插件市场或抽象到所有 Agent CLI。只有第一条
多人闭环真实可用后，才用第二个 Runtime 验证适配边界。

本设计取代以下内容的近期优先级，但保留其中已经实施且仍有效的工程成果：

- [2026-08-24 总体架构](2026-08-24-rivloom-desktop-architecture-design.md)中“先完成单机
  Chat 客户端，再建设协作”的产品顺序。
- PR #37 的 A3 Chat 页面方案及 PR #38 的 A3.0 有界历史方案，不再作为当前开发前置。
- A2 文档中“下一阶段必须先完成完整 thread 恢复和历史装载”的表述。

## 2. 产品目标

Rivloom 的核心价值是让多人把本地 Agent 能力组织成一个可理解、可授权、可追踪的
协作网络，而不是再提供一套某个 Runtime 已经拥有的对话界面。

第一条纵向切片必须证明：

1. Alice 和 Bob 在两台 Windows 电脑上分别运行 Rivloom。
2. 两台电脑各自完成 Codex Runtime 登录并登记本地项目。
3. 两人通过邀请和 Tailscale，或启用应用层 TLS 的 LAN，加入同一个最小 Brain。
4. Alice 将一个有界编程任务委派给 Bob 的 Node。
5. Bob 明确接受并把任务映射到自己的本地项目。
6. Bob 的 Node 在隔离 worktree 中拉起 Codex 完成任务。
7. 双方只看到必要且有界的运行状态。
8. Bob 返回摘要、测试结果和 Patch；Alice 可以接受或拒绝。

这个闭环成功之前，不以支持更多 Runtime、更多 Brain 或完整聊天体验作为成功指标。

## 3. 现状判断与保留范围

当前 `apps/desktop` 已经采用正确的外部进程边界：

- Tauri 监管独立 `codex-app-server`，没有依赖任何 `codex-*` crate。
- 构建脚本支持用 `RIVLOOM_APP_SERVER_PATH` 指向外部二进制。
- App Server 使用 Rivloom 专属 `CODEX_HOME`，与官方 Codex 数据隔离。
- A1 已提供浏览器 ChatGPT 登录、账号状态和退出登录。
- A2 已提供受控目录选择、最近项目以及稳定的 thread `cwd` 映射。

这些实现不推倒重来，而是重新归属：

| 已有能力 | 新架构中的归属 | 处理方式 |
| --- | --- | --- |
| App Server 进程监管与 JSONL 连接 | `CodexRuntime` | 直接复用，收敛成 Codex 专用适配层 |
| ChatGPT 登录与独立 `CODEX_HOME` | Codex Runtime Auth | 保留实现，修改产品文案和领域边界 |
| 最近项目与稳定 `cwd` | Node 的本地资源映射 | 保留；Brain 只见不透明项目引用 |
| thread 列表与创建 | Codex 运行上下文 | 按任务执行需要复用，不再扩成完整 Chat 产品 |
| 现有 React 外壳、卡片和设计 token | Rivloom 控制面 UI | 渐进加入任务、Node、审查，不重画整套页面 |

## 4. 边界：Rivloom 拥有什么

### 4.1 Rivloom Identity

Rivloom 用户与设备身份用于邀请、成员关系、Node 认证、委派和审查。它不等同于任何
Runtime 账号。首版可以是本地生成的设备密钥、显示名和 Brain 成员记录，不要求先建设
中心化 SaaS 账号。

### 4.2 Node

Node 是可以接受任务并在本机执行的 Rivloom 实例。它拥有：

- 本地 Runtime 及其认证状态。
- 本地项目路径和不透明项目映射。
- worktree、沙箱、审批与执行环境。
- Artifact 的生成、大小限制和上传决定。

Node 的绝对路径、Runtime Token 和工作区内容不是 Brain 数据。

### 4.3 Task、Assignment 与 Run

- `Task`：人可读的目标、约束、期望产物和有界上下文。
- `Assignment`：任务被提供给哪个 Node、由谁接受及具有什么权限。
- `Run`：Node 上一次具体 Runtime 执行，可中断且有唯一幂等键。

任务主状态采用显式状态机：

```text
draft -> offered -> accepted -> running -> awaitingReview
   |        |          |          |               |
   +------> cancelled  +--------> failed          +-> approved
                                  |               +-> rejected
                                  +-> outcomeUnknown
```

`outcomeUnknown` 表示连接中断后无法证明远端执行是否完成。Rivloom 不自动重复执行可能
产生文件或外部副作用的任务；必须先由执行 Node 对账，或由人明确重新运行。

### 4.4 Artifact 与 RunReceipt

首版 Artifact 只需要支持：

- 有界文本摘要。
- 执行过的测试及其退出结果。
- 有界 Patch 或 Patch 引用、摘要哈希和大小。
- 警告、失败原因和需要人工处理的审批结果。

每次 Run 结束生成 `RunReceipt`，至少包含 task/run/node/runtime 标识、Runtime 版本、
开始与结束时间、结果状态、测试摘要、Patch 摘要与内容哈希。完整日志默认留在执行 Node，
Brain 只保存有界摘要和 Artifact 元数据。

### 4.5 Permission

权限必须在三个时点明确：

1. Alice 委派时声明任务目标和可接受产物。
2. Bob 接受时选择本地项目并确认 Node 可执行。
3. Codex 运行时继续遵守本机沙箱和危险操作审批。

远端委派不能绕过执行 Node 的本地审批。第一版不提供“远端永久允许所有命令”。

## 5. 外部 Runtime 边界

### 5.1 Codex 是第一个 Runtime，不是 Rivloom 内核

```text
Rivloom Desktop / Node
  ├─ Rivloom Identity
  ├─ Local Project Registry
  ├─ Task / Artifact / Review UI
  └─ Codex Runtime Adapter
       └─ codex-app-server sidecar
            └─ local ChatGPT/Codex auth

Node A ───────┐
              ├── Minimal Brain
Node B ───────┘    membership / presence / task status
```

Rivloom 不实现模型循环、工具选择、上下文压缩或 Codex 沙箱。`CodexRuntime` 只负责：

- 进程发现、版本检查、启动、监控和停止。
- Runtime 认证状态。
- 把本地 Task Run 映射为必要的 `thread/*`、`turn/*` 和中断调用。
- 将流式事件归一化为少量任务状态。
- 收集 Diff、测试摘要和运行结果形成 `RunReceipt`。

优先使用结构化协议。PTY/CLI 只能作为某个未来 Runtime 没有可靠结构化接口时的降级，
不得让终端输出解析成为整个产品的核心协议。

### 5.2 不提前设计“万能 RuntimeAdapter”

首版代码可以建立清晰的 Codex 边界和少量内部数据结构，但不为尚未接入的 Claude Code、
Hermes、Reasonix 猜测统一能力。第二个 Runtime 接入时，再从两个真实实现中提取最小
公共契约，例如：

- `capabilities`
- `auth_status`
- `start_run`
- `interrupt_run`
- `subscribe_events`
- `collect_artifacts`

Runtime 特有能力通过声明和可选字段表达，不能假装所有 Runtime 都有 thread、Diff、
审批或相同登录方式。

## 6. Brain 与网络范围

第一版 Brain 是单一权威协调点，可以由其中一台 Rivloom 实例承担。它只负责：

- 邀请和成员关系。
- Node 注册、能力摘要、心跳与在线状态。
- Task、Assignment、Run 状态和审查决定。
- Artifact 元数据和受限传输协调。

第一版只承诺 Tailscale 等已加密私网，或启用应用层 TLS 的 LAN；不允许在普通 LAN
明文传输任务，也不自研 NAT 穿透、P2P 大文件传输、共识协议或多 Brain 选主。Node 与
Brain 的长连接必须使用设备身份认证、消息版本、幂等键和有界 payload。断线重连先
对账，不依据最后一个 UI 状态猜测执行结果。

## 7. 关键数据边界

| 数据 | Node 本地 | Brain | 其他成员默认可见 |
| --- | --- | --- | --- |
| Runtime OAuth/API Token | 是 | 否 | 否 |
| 本机绝对项目路径 | 是 | 否 | 否 |
| 项目不透明 ID/显示名 | 是 | 是 | 按任务可见 |
| Task 目标与约束 | 是 | 是 | 参与者可见 |
| 完整 Runtime 对话/日志 | 是 | 否 | 否 |
| 有界运行状态 | 是 | 是 | 参与者可见 |
| Patch/测试/摘要 | 是 | 元数据或有界内容 | 审查者可见 |
| 设备公钥与成员角色 | 是 | 是 | Brain 成员可见 |

任务内容本身可能包含敏感信息。UI 必须在委派前明确展示将发送的内容；Node 接受前展示
任务来源和请求权限。日志与协议错误不得包含 Token、完整环境变量、绝对路径或未截断的
Runtime payload。

## 8. 首个 Codex 任务执行映射

首版不需要先恢复完整 Chat 历史。一次任务执行的最小映射为：

1. Node 解析本地项目映射并创建隔离 worktree。
2. `CodexRuntime` 使用该 worktree 的 `cwd` 创建或恢复专用 thread。
3. Node 发送一次明确的 `turn/start`，任务正文包含目标、约束和期望回执。
4. 适配层把 App Server 事件压缩成 `queued/running/waitingApproval/completed/failed`。
5. 本地用户处理仍由 Codex 发起的审批；远端只看到“等待本地处理”。
6. 完成后 Node 从 worktree 收集 Patch、测试结果与有界摘要。
7. Node 生成 `RunReceipt` 并上传允许共享的 Artifact。

任务详情可以展示有限的事件时间线，但不把无限对话历史或原始 JSONL 同步给 Brain。

## 9. UI 演进

不重新设计整套桌面页面。沿用现有 AppShell、状态卡片、项目选择和设计 token，按以下
顺序渐进增加：

1. 将“ChatGPT 账号”明确标注为“Codex Runtime 登录”。
2. 在相邻区域加入 Rivloom 本地身份和当前 Brain。
3. 将项目工作区的主入口从“完整聊天”改为“新建任务”。
4. 增加任务列表、Node 选择、委派确认和有限状态时间线。
5. 增加 Patch/测试摘要审查页。

完整 Chat 可以以后作为调试或 Runtime 详情视图存在，但不是第一条协作闭环的页面骨架。

## 10. 安全、授权与许可证门禁

### 10.1 安全门禁

- Brain 不保存 Runtime 凭证，不代替 Node 登录 Runtime。
- 执行必须绑定一次接受记录、项目映射、Run ID 和幂等键。
- 默认在隔离 worktree 运行；不能静默写入用户当前 checkout。
- Artifact 有单项和总量硬上限，并校验内容哈希。
- 未知协议版本、身份不匹配或签名失败时拒绝降级。
- 高风险本地审批不允许由远端自动确认。

### 10.2 Runtime 许可证门禁

当前仓库和 Codex 的适用开源代码采用 Apache-2.0。以独立进程捆绑
`codex-app-server` 仍需要随发行物保留适用的 `LICENSE`、`NOTICE`、版权和修改说明，
并生成第三方依赖清单与 SBOM；进程边界不等于免除再分发义务。

每增加一个 Runtime，必须先形成版本固定的接入审查，至少确认：

- 源码许可证是否允许修改、再分发和商业使用。
- CLI/二进制是否允许随 Rivloom 捆绑，还是只能由用户自行安装。
- 自动化调用、账号登录和凭证存储是否符合服务条款。
- 商标、品牌和“官方客户端”表述限制。
- 依赖许可证、NOTICE、源代码提供义务和 SBOM 处理。

Claude Code、Hermes、Reasonix 等在进入里程碑前分别审查当时固定版本；不能用项目名称或
历史许可证推断未来版本条款。若不能安全再分发，适配器只能发现用户自行安装的可执行
文件，并在产品中清楚标注第三方归属。

本节是工程发布门禁，不替代正式法律意见。

## 11. 失败模式

| 故障 | 必须表现 | 处理原则 |
| --- | --- | --- |
| Runtime 未登录或 sidecar 缺失 | Node 不可执行 | 本机修复，不把凭证发给 Brain |
| Bob 拒绝或无项目映射 | Task 保持未执行 | Alice 可撤回或改派 |
| Brain 断线但 Node 未开始 | 保持 accepted/queued | 重连对账后再开始 |
| 运行中断线 | `outcomeUnknown` | 不自动重跑，等待 Node 回执 |
| Codex 需要审批 | `waitingApproval` | 由 Bob 本地处理 |
| App Server 崩溃 | Run 失败或未知 | 保留 worktree 和诊断，显式重试 |
| Patch 超限或校验失败 | Artifact 不可审查 | 仅回传摘要，要求本地或分片处理 |
| Node 重复收到请求 | 不重复执行 | 按 Run 幂等键返回已有状态 |
| 审查后基线已变化 | 不直接应用 | 重新校验或重新生成 Patch |

## 12. 测试策略

首条闭环至少覆盖：

- Codex Runtime 认证与 Rivloom Identity 在类型、存储和 UI 上完全分离。
- Runtime Token、绝对路径和完整日志不会出现在 Brain 协议或快照中。
- Task 状态机和重复消息具有确定性，非法跳转被拒绝。
- 断线恢复进入对账流程，运行中的任务不会被自动重复执行。
- 两个临时 Node 与一个测试 Brain 的端到端委派。
- fake Codex App Server 下的 `turn/start`、事件、中断、审批和完成回执。
- worktree 隔离、Patch 大小限制、内容哈希和基线变化。
- Windows 两机 Tailscale 或启用应用层 TLS 的 LAN 手工验收。

## 13. 明确非目标

以下内容在第一条多人闭环完成前不启动：

- 嵌入或 fork `codex-core` 作为 Rivloom 内核。
- 同时支持 Codex、Claude Code、Hermes、Reasonix。
- 通用 Agent harness、Marketplace 或 Skill Directory。
- 完整 Chat 历史同步、无限事件流或跨 Runtime 会话迁移。
- 公网 SaaS 控制面、NAT 穿透、P2P 大文件传输。
- Raft、多 Brain、自动选主和跨区域高可用。
- 无人值守地替远端用户批准危险命令。

## 14. 里程碑 Gate

| Gate | 结果，不以代码量衡量 |
| --- | --- |
| R0 方向收敛 | 新设计生效，旧 A3 PR 暂停，历史文档有明确指向 |
| R1 边界分离 | UI 和存储能同时表达 Rivloom Identity 与 Codex Runtime Auth |
| R2 本地任务闭环 | 单机从 Task 到 Codex RunReceipt，不依赖完整 Chat 页面 |
| R3 最小 Brain | 两个 Node 可邀请加入、认证、报告能力与在线状态 |
| R4 远端委派 | Alice 委派，Bob 接受并执行，断线不重复运行 |
| R5 Artifact 审查 | Alice 可校验摘要、测试和 Patch 后接受或拒绝 |
| R6 第二 Runtime 验证 | 通过许可证门禁后，用一个真实 Runtime 提取最小公共契约 |
| R7 Skill Directory | 仅在至少两个 Runtime 的真实任务需要后评审 |

详细实施顺序见
[Runtime Host Transition Implementation Plan](2026-08-30-runtime-host-transition-plan.md)。

## 15. 第一版验收标准

在两台受支持的 Windows 电脑上：

1. 两台电脑安装 Rivloom，不要求用户安装 Rust 或 Node.js。
2. 两台电脑分别完成 Codex Runtime 登录；一个人的 Runtime 凭证不能被另一台读取。
3. Alice 发出一次性邀请，Bob 通过 Tailscale 或启用应用层 TLS 的 LAN 加入同一 Brain。
4. Alice 向 Bob Node 委派一个有明确目标、权限和产物的任务。
5. Bob 明确接受并选择自己的本地项目映射。
6. Bob Node 在隔离 worktree 中运行 Codex。
7. 双方看到有界且一致的任务状态；审批只在 Bob 本地完成。
8. Bob 返回摘要、测试结果和带内容哈希的有界 Patch。
9. Alice 能基于可见 Artifact 接受或拒绝结果。
10. Brain 中没有 OAuth Token、本机绝对路径、完整工作区或无限日志。
11. 运行中断线进入 `outcomeUnknown`，没有自动重复执行。

## 16. 相关决策

- [ADR-0001：采用 Tauri、React 与 App Server sidecar](../adr/0001-use-tauri-react-and-app-server-sidecar.md)
- [ADR-0002：隔离 Rivloom 的 Codex 数据目录](../adr/0002-isolate-rivloom-codex-home.md)
- [ADR-0003：分离 Rivloom 产品代码并以合并方式同步上游](../adr/0003-separate-rivloom-code-from-upstream-codex.md)
- [ADR-0004：以稳定 cwd 协议表示本地项目](../adr/0004-use-stable-cwd-for-local-projects.md)
- [ADR-0005：采用外部 Agent Runtime](../adr/0005-use-external-agent-runtimes.md)
- [ADR-0006：分离 Rivloom Identity 与 Runtime Auth](../adr/0006-separate-rivloom-identity-from-runtime-auth.md)
