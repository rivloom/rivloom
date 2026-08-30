# Rivloom Runtime Host Transition Implementation Plan

> **执行方式：** 按 Task 分批实施，每批完成后先验证和审查，再进入下一批；不要跨 Gate
> 并行堆叠未验证功能。

**Goal:** Reframe the existing Rivloom Desktop as a Codex Runtime host and deliver the smallest secure two-person, two-Node task delegation and Patch review loop without embedding Codex internals or rebuilding the current UI.

**Architecture:** Keep the existing Tauri/React desktop, isolated `CODEX_HOME`, local project registry, and supervised `codex-app-server` process. Add Rivloom-owned identity, task, Node, Brain, RunReceipt, and Artifact boundaries around a concrete Codex runtime path. Implement one vertical slice before extracting a generic adapter or adding a second runtime.

**Tech Stack:** Rust 2024, Tauri 2, React 19, TypeScript 5, App Server v2 JSONL, Vitest, Rust unit/integration tests, Git worktrees, and an authenticated encrypted private-network connection for Node-to-Brain traffic.

---

## 1. 执行约束

- 不修改 `codex-rs`，不依赖 `codex-core` 或其他 `codex-*` crate。
- 不合并 PR #37/#38，也不把完整 Chat 历史作为协作闭环前置。
- 复用 `apps/desktop` 的 App Server、账号、项目和现有 UI 组件。
- 先写失败测试，再实现最小行为；每个 PR 保持可独立审查和回滚。
- 单个非机械 PR 控制在 800 行以内，复杂逻辑尽量低于 500 行。
- 所有跨进程和跨 Node payload 必须有硬上限、版本号和脱敏错误。
- 不允许跨网传输 Runtime Token、本机绝对路径、完整环境变量或无限日志。
- 每个阶段的 Gate 未通过时，不开始后续 Runtime 或 Marketplace 工作。
- UI 变化必须补充现有 React snapshot/组件覆盖；Rust 变更按仓库规则运行
  `just fmt`、项目测试和有针对性的 lint。
- 能在未修改分支的 `origin/main` 上复现的全仓 CI 基线故障单独跟踪；不能把它伪装成
  当前 PR 通过，也不能把无关基线修复混入当前功能 PR。当前 PR 仍必须通过全部定向检查。

## 2. 当前基线

截至 `origin/main` 的 `5a8f909bb7f9b10e9a0298bb7564f92c578862a6`（PR #39）：

- A0 桌面外壳和 App Server 监管已完成。
- A1 浏览器 ChatGPT 登录和独立 `CODEX_HOME` 已完成。
- A2 本地最近项目、稳定 `cwd` thread list/start/read 已完成。
- Runtime Host 权威设计、ADR-0005/0006 和历史路线重定向已经进入 `main`。
- PR #37/#38 已转为 Draft 并明确暂停，分支和 worktree 保留作研究材料。
- `turn/start`、流式 Run、worktree Artifact、Brain、Node 和多人委派尚未进入 `main`。

历史 A3 分支只作为实现研究材料，不是迁移计划依赖。若复用其中代码，必须重新按本计划
切成最小 PR，并证明没有带入完整 Chat 历史或无界事件模型。

## 3. R0：路线与开发队列收尾

### Task R0.1：发布权威设计并暂停旧路线（已完成）

- PR #39 已将 Runtime Host 设计、ADR-0005/0006 和历史文档重定向合并到 `main`。
- PR #37/#38 已标记为 Draft 并留言说明暂停；分支和 worktree 未删除。
- 权威设计 PR 经拆分后为 556 行，未把详细实施计划混入架构决策审查。

### Task R0.2：单独发布迁移实施计划

**Files:**

- Add: `rivloom-docs/plans/2026-08-30-runtime-host-transition-plan.md`
- Modify: `rivloom-docs/README.md`
- Modify: `rivloom-docs/plans/2026-08-24-rivloom-desktop-architecture-design.md`
- Modify: `rivloom-docs/plans/2026-08-30-runtime-host-collaboration-design.md`

**Steps:**

1. 将本计划作为权威设计的配套实施顺序发布，不修改已接受的架构边界。
2. 补齐 R2、R3、R5 和 R6 的纵向集成测试及传输安全门禁。
3. 更新文档索引和新旧设计的交叉引用。
4. 运行链接、空白、规模和差异校验。

**Verification:**

```powershell
git diff --check
rg -n "Transition Implementation Plan|Task R1.1|Gate R6" rivloom-docs
git diff --stat origin/main...HEAD
git diff -- rivloom-docs
```

**Commit:**

```powershell
git add rivloom-docs
git commit -m "docs: publish runtime host transition plan"
```

**Gate R0:**

- `main` 有且只有一个当前权威路线入口。
- #37/#38 明确暂停且可恢复。
- 权威设计与迁移计划分别可独立审查，均不含产品代码。

## 4. R1：分离 Rivloom Identity 与 Codex Runtime Auth

### Task R1.1：锁定两个领域的前后端契约

**Files:**

- Add: `apps/desktop/src-tauri/src/identity/mod.rs`
- Add: `apps/desktop/src-tauri/src/identity/types.rs`
- Add: `apps/desktop/src-tauri/src/identity/types_tests.rs`
- Add: `apps/desktop/src/types/identity.ts`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src/types/account.ts`

**Test first:**

- `RivloomIdentity` 只包含本地 identity ID、display name、device ID 和 Brain membership 摘要。
- 现有 account DTO 显式命名为 Codex Runtime auth，不出现 Brain role 或 member ID。
- 序列化快照证明两个对象不能互相替代。

**Implementation:**

建立最小具体类型，不引入通用身份提供商。可以暂时保留 Rust `account` 模块名以减少代码
搬迁，但其公开 DTO 和 UI 文案必须表达 Codex Runtime auth。

**Verification:**

```powershell
cd apps/desktop
npm test
cd src-tauri
cargo test identity
cargo test account
```

### Task R1.2：持久化本地 Rivloom Identity

**Files:**

- Add: `apps/desktop/src-tauri/src/identity/storage.rs`
- Add: `apps/desktop/src-tauri/src/identity/storage_tests.rs`
- Add: `apps/desktop/src-tauri/src/identity/service.rs`
- Add: `apps/desktop/src-tauri/src/identity/service_tests.rs`
- Modify: `apps/desktop/src-tauri/src/identity/mod.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Test first:**

- 首次启动生成稳定 device ID 和本地 identity ID。
- display name 有 UTF-8 字节上限并经过空白规范化。
- 损坏和未来版本文件不会被静默覆盖。
- 存储中没有 Codex Token、`CODEX_HOME` 内容或本机项目路径。

**Implementation:**

沿用最近项目存储的版本化、原子替换和损坏隔离模式。设备认证密钥的生成与保护留给 R3，
本任务只建立非秘密 identity 元数据。

### Task R1.3：在现有首页同时展示两种状态

**Files:**

- Add: `apps/desktop/src/components/IdentityCard/IdentityCard.tsx`
- Add: `apps/desktop/src/components/IdentityCard/IdentityCard.module.css`
- Add: `apps/desktop/src/components/IdentityCard/IdentityCard.test.tsx`
- Add: `apps/desktop/src/lib/identityBridge.ts`
- Add: `apps/desktop/src/lib/identityBridge.test.ts`
- Add: `apps/desktop/src/hooks/useIdentity.ts`
- Add: `apps/desktop/src/hooks/useIdentity.test.tsx`
- Modify: `apps/desktop/src/components/AccountAccessCard/AccountAccessCard.tsx`
- Modify: `apps/desktop/src/components/AccountAccessCard/AccountAccessCard.test.tsx`
- Modify: `apps/desktop/src/content/zh-CN.ts`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`

**Test first:**

- 首页分别显示“Rivloom 身份”和“Codex Runtime”。
- 未完成 Codex 登录不影响本地 identity 的存在。
- Runtime 登录成功不能伪装为已加入 Brain。
- 继续使用现有布局和 token，不重做 AppShell。
- App 组合测试证明 Identity 与 Runtime Auth 分别接线，任一状态变化不会覆盖另一个状态。

**Gate R1:**

- 用户和代码都能清楚区分 Rivloom Identity、Node 与 Codex Runtime Auth。
- Brain 数据模型没有 Runtime 凭证字段。

## 5. R2：单机 Codex Task Run 与 RunReceipt

### Task R2.1：建立有界 Task 与 Run 状态机

**Files:**

- Add: `apps/desktop/src-tauri/src/task/mod.rs`
- Add: `apps/desktop/src-tauri/src/task/types.rs`
- Add: `apps/desktop/src-tauri/src/task/state_machine.rs`
- Add: `apps/desktop/src-tauri/src/task/state_machine_tests.rs`
- Add: `apps/desktop/src/types/task.ts`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Test first:**

- 对设计文档中的所有合法状态转换做整对象比较。
- 非法转换、重复完成和未知 Run ID 被拒绝。
- goal、constraints、summary、error 和 event 数量均有硬上限。
- 运行中断线只能进入 `outcomeUnknown`，不能跳到 queued 自动重试。

### Task R2.2：建立版本化本地 Task Store

**Files:**

- Add: `apps/desktop/src-tauri/src/task/storage.rs`
- Add: `apps/desktop/src-tauri/src/task/storage_tests.rs`
- Add: `apps/desktop/src-tauri/src/task/service.rs`
- Add: `apps/desktop/src-tauri/src/task/service_tests.rs`
- Modify: `apps/desktop/src-tauri/src/task/mod.rs`

**Test first:**

- 最多保留固定数量 Task 和每个 Task 的固定数量 event。
- 幂等键重复写入返回原 Task/Run。
- 原子写入失败保留旧文件。
- 存储序列化不包含本机绝对路径或 Runtime Token。

### Task R2.3：增加最小 Codex Run 事件路由

**Files:**

- Add: `apps/desktop/src-tauri/src/runtime/mod.rs`
- Add: `apps/desktop/src-tauri/src/runtime/codex.rs`
- Add: `apps/desktop/src-tauri/src/runtime/codex_tests.rs`
- Add: `apps/desktop/src-tauri/src/app_server/event_router.rs`
- Add: `apps/desktop/src-tauri/src/app_server/event_router_tests.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/mod.rs`
- Modify: `apps/desktop/src-tauri/src/app_server/connection.rs`

**Test first:**

- fake App Server 的 `turn/start` 请求只使用后端登记的 `cwd`。
- 只归一化 Run 需要的状态、审批等待、完成和失败事件。
- 慢观察者不会阻塞 App Server reader；缓冲区满时产生明确 gap 状态。
- 单 Run 事件数、单事件和总字节数均有上限。
- interrupt 只作用于匹配的活动 Run。

**Implementation:**

不要引入完整 Chat message tree、无限 turn 历史或通用事件总线。若参考 PR #38，只复用经过
重新评审的有界分发思想和测试，不把该 PR 作为依赖。

### Task R2.4：隔离 worktree 并收集 Patch Artifact

**Files:**

- Add: `apps/desktop/src-tauri/src/task/worktree.rs`
- Add: `apps/desktop/src-tauri/src/task/worktree_tests.rs`
- Add: `apps/desktop/src-tauri/src/task/artifact.rs`
- Add: `apps/desktop/src-tauri/src/task/artifact_tests.rs`
- Modify: `apps/desktop/src-tauri/src/task/mod.rs`
- Modify: `apps/desktop/src-tauri/src/task/service.rs`

**Test first:**

- 只对已登记且确认为 Git 仓库的项目创建专属 worktree。
- worktree 目标必须位于 Rivloom 管理的精确目录内。
- 当前 checkout 和用户未提交修改不被覆盖。
- Patch 有字节上限、SHA-256、基线 commit 和截断/超限状态。
- 清理失败保留可诊断记录，不递归删除未验证路径。

### Task R2.5：生成 RunReceipt 并提供本地任务 UI

**Files:**

- Add: `apps/desktop/src-tauri/src/task/receipt.rs`
- Add: `apps/desktop/src-tauri/src/task/receipt_tests.rs`
- Add: `apps/desktop/src-tauri/tests/local_codex_task_run.rs`
- Add: `apps/desktop/src-tauri/tests/support/mod.rs`
- Add: `apps/desktop/src-tauri/tests/support/fake_codex.rs`
- Add: `apps/desktop/src/components/TaskComposer/TaskComposer.tsx`
- Add: `apps/desktop/src/components/TaskComposer/TaskComposer.test.tsx`
- Add: `apps/desktop/src/components/TaskRun/TaskRun.tsx`
- Add: `apps/desktop/src/components/TaskRun/TaskRun.test.tsx`
- Add: `apps/desktop/src/lib/taskBridge.ts`
- Add: `apps/desktop/src/lib/taskBridge.test.ts`
- Add: `apps/desktop/src/hooks/useTasks.ts`
- Add: `apps/desktop/src/hooks/useTasks.test.tsx`
- Modify: `apps/desktop/src/components/ProjectWorkspace/ProjectWorkspace.tsx`
- Modify: `apps/desktop/src/components/ProjectWorkspace/ProjectWorkspace.test.tsx`
- Modify: `apps/desktop/src/components/ProjectWorkspace/__snapshots__/ProjectWorkspace.test.tsx.snap`
- Modify: `apps/desktop/src/content/zh-CN.ts`

**Test first:**

- `receipt_tests.rs` 对 success、failed、cancelled 和 `outcomeUnknown` 的完整对象做相等比较，
  覆盖 task/run/node/runtime ID、Runtime 版本、Unix 秒时间戳、终态、测试摘要、Patch 与
  baseline hash、字段上限，并证明序列化结果不含 Token 和绝对路径。
- `local_codex_task_run.rs` 从已登记项目和稳定 `cwd` 开始，经过专用 thread、唯一一次
  `turn/start`、事件路由和 Task service，最终生成完整 RunReceipt。
- 同一纵向测试覆盖成功、审批只在本地等待、用户停止、App Server 断线进入
  `outcomeUnknown`、Runtime 失败和事件 gap，并核对 thread/run correlation。
- 重复幂等请求和重连对账不能发出第二次 `turn/start`；错误或超限任务在调用 Runtime
  前被拒绝。
- 用户从现有项目页创建 Task，而不是先进入完整 Chat。
- UI 只显示有界时间线、审批等待、测试摘要和 Patch 摘要。
- 页面卸载或 App Server 断线不会显示虚假成功。
- RunReceipt 缺少 Runtime 报告的测试时明确显示“未报告”，不猜测通过。
- `taskBridge.test.ts` 校验命令名、参数和 thread/run correlation，不让错误接线被组件 mock 掩盖。
- `useTasks.test.tsx` 校验订阅清理、项目切换后的旧事件隔离，以及断线不会留下成功状态。
- `ProjectWorkspace` 组合测试和 snapshot 覆盖从现有项目页创建 Task、Run 状态和回执的接线。

**Gate R2:**

- 单机能够从本地 Task 启动 Codex、看到有限状态、停止 Run，并得到可校验 RunReceipt。
- 没有完整 Chat 页面也能完成这条闭环。

## 6. R3：最小 Brain 与两个 Node

### Task R3.1：冻结协作协议 v1

**Files:**

- Add: `apps/desktop/src-tauri/src/collaboration/mod.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/protocol.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/protocol_tests.rs`
- Add: `rivloom-docs/plans/collaboration-protocol-v1.md`

**Test first:**

- 消息使用显式 `protocolVersion`、message ID、幂等键和 Unix 秒时间戳。
- 身份、Node、Task、Assignment、RunReceipt 和 Artifact metadata 都有有界 schema。
- 反序列化未知版本直接拒绝。
- golden payload 不含绝对路径、Token、完整日志或 App Server 原始消息。

### Task R3.2：实现邀请、成员和 Node 凭证

**Files:**

- Add: `apps/desktop/src-tauri/src/collaboration/credential.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/credential_tests.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/invitation.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/invitation_tests.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`
- Modify if produced by the required lock update: `MODULE.bazel.lock`

**Test first:**

- 邀请一次性使用、短时有效并绑定 Brain。
- Brain 只保存凭证哈希或公钥，不保存明文长期 secret。
- 成员撤销后连接和新 Task 均被拒绝。
- 日志和错误不回显邀请 secret。

**Implementation note:**

首版网络支持应优先要求 Tailscale 等已加密私网；若支持普通 LAN，必须在同一 Gate 内提供
经过审查的 TLS/证书固定，不能发送明文任务。不要自创密码学协议。

### Task R3.3：实现单 Brain 的成员、presence 和状态存储

**Files:**

- Add: `apps/desktop/src-tauri/src/collaboration/brain.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/brain_tests.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/storage.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/storage_tests.rs`
- Modify: `apps/desktop/src-tauri/src/collaboration/mod.rs`

**Test first:**

- 单 Brain 对成员、Node 和 Task 状态拥有唯一修订号。
- 心跳有过期状态，但离线不等于任务失败。
- 重复/乱序消息不回退状态。
- 存储损坏不会导致 Brain 猜测 Run 成功。

### Task R3.4：实现 Node 客户端和对账

**Files:**

- Add: `apps/desktop/src-tauri/src/collaboration/node.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/node_tests.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/reconcile.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/reconcile_tests.rs`
- Add: `apps/desktop/src-tauri/tests/collaboration_transport.rs`

**Test first:**

- 两个临时 Node 能认证到一个测试 Brain 并报告有界 capabilities。
- 断线后使用最后修订号增量对账。
- running Run 断线标记 `outcomeUnknown`，不会再次调用 Codex。
- 已完成 Run 重发相同 RunReceipt，内容哈希保持一致。
- 普通 LAN endpoint 未配置应用层 TLS 和 pinned peer identity 时在发送 Task 前被拒绝，
  且不允许失败后降级为明文。
- 错误或未信任的证书、公钥 pin 不匹配和过期凭证均无法连接；正确 pinned peer 可以
  认证、对账并交换有界消息。

**Gate R3:**

- 两台 Windows 设备可通过受支持的加密私网和邀请加入同一 Brain。
- Brain 只能看到设计允许的数据。

## 7. R4：两 Node 远端任务委派

### Task R4.1：实现 offer、accept、reject 与本地项目映射

**Files:**

- Add: `apps/desktop/src-tauri/src/collaboration/assignment.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/assignment_tests.rs`
- Modify: `apps/desktop/src-tauri/src/project/service.rs`
- Modify: `apps/desktop/src-tauri/src/task/service.rs`

**Test first:**

- Alice 的 Task 不携带她的绝对路径。
- Bob 必须明确接受并选择自己的已登记项目，才允许创建 Run。
- 拒绝、过期、撤回和重复接受行为确定。
- Runtime 未登录或项目不可用时 Node 能说明不可执行但不泄露路径。

### Task R4.2：增加 Node 与委派 UI

**Files:**

- Add: `apps/desktop/src/components/BrainCard/BrainCard.tsx`
- Add: `apps/desktop/src/components/BrainCard/BrainCard.test.tsx`
- Add: `apps/desktop/src/components/NodePicker/NodePicker.tsx`
- Add: `apps/desktop/src/components/NodePicker/NodePicker.test.tsx`
- Add: `apps/desktop/src/components/AssignmentInbox/AssignmentInbox.tsx`
- Add: `apps/desktop/src/components/AssignmentInbox/AssignmentInbox.test.tsx`
- Modify: `apps/desktop/src/components/TaskComposer/TaskComposer.tsx`
- Modify: `apps/desktop/src/components/TaskComposer/TaskComposer.test.tsx`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/content/zh-CN.ts`

**Test first:**

- Alice 委派前能看到目标 Node、Runtime 能力和将发送的任务内容。
- Bob 接受前能看到来源、权限和期望 Artifact。
- 离线/撤销 Node 不可被新选中。
- 所有操作沿用现有 AppShell，不引入第二套导航或设计系统。
- `TaskComposer` 与 App 组合测试覆盖 Node 选择、委派入口和本地执行入口之间的接线。

### Task R4.3：两 Node 端到端测试

**Files:**

- Add: `apps/desktop/src-tauri/tests/two_node_delegation.rs`
- Reuse: `apps/desktop/src-tauri/tests/support/fake_codex.rs`

**Scenarios:**

1. Alice offer -> Bob accept -> fake Codex complete -> Alice receives receipt。
2. Bob reject -> Alice sees final rejected state。
3. running 时断线 -> outcomeUnknown -> Bob 对账完成 -> 不重复 start。
4. 重复 offer/receipt -> exactly-once observable result。
5. 恶意超限 payload -> 连接拒绝且 Brain 保持可用。

**Gate R4:**

- Alice 和 Bob 可以在两台 Node 上完成真实 Codex 任务委派，且断线不会自动重复执行。

## 8. R5：Patch 与测试结果审查

### Task R5.1：实现 Artifact 传输与完整性校验

**Files:**

- Add: `apps/desktop/src-tauri/src/collaboration/artifact_transfer.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/artifact_transfer_tests.rs`
- Modify: `apps/desktop/src-tauri/src/task/artifact.rs`
- Modify: `apps/desktop/src-tauri/src/collaboration/protocol.rs`

**Test first:**

- 单 Artifact、单 Task 和并发传输都有硬上限。
- 分块内容按 SHA-256 校验，缺块或错序不能成为可审查 Artifact。
- 超限时只传摘要并明确标记，不能静默截断成看似完整 Patch。
- 临时文件位于经验证的 Rivloom 管理目录，失败可恢复清理。

### Task R5.2：实现审查决定和基线保护

**Files:**

- Add: `apps/desktop/src-tauri/src/collaboration/review.rs`
- Add: `apps/desktop/src-tauri/src/collaboration/review_tests.rs`
- Add: `apps/desktop/src/components/ArtifactReview/ArtifactReview.tsx`
- Add: `apps/desktop/src/components/ArtifactReview/ArtifactReview.test.tsx`
- Modify: `apps/desktop/src/types/task.ts`
- Modify: `apps/desktop/src/content/zh-CN.ts`
- Modify: `apps/desktop/src-tauri/tests/two_node_delegation.rs`

**Test first:**

- Alice 能查看来源 Node、Runtime 版本、基线、测试和 Patch 哈希。
- approve/reject 使用幂等 review ID 并记录审查者。
- 基线不匹配时阻止自动应用并提示重新生成或手工处理。
- 首版 approve 只确认协作结果，不默认向 Alice 工作区自动写入 Patch。
- 两 Node 集成测试把 fake Codex 的 Patch、测试摘要和 RunReceipt 经 Brain 传给 Alice，
  校验内容 hash、大小和来源；缺块、损坏、错序和超限 Artifact 均不能进入可审查状态。
- 同一集成测试覆盖幂等 approve/reject、baseline mismatch，并证明 approve 后 Alice 的
  当前 checkout 没有被写入。

### Task R5.3：Windows 两机验收与威胁复查

**Files:**

- Add: `rivloom-docs/plans/two-node-codex-slice-verification.md`
- Modify: `rivloom-docs/plans/2026-08-30-runtime-host-collaboration-design.md` only if findings change the accepted design

**Manual checklist:**

执行当前权威设计第 15 节的全部 11 项验收，并额外导出 Brain 数据、Node 日志和网络
抓包，确认没有 Runtime Token、绝对路径或完整工作区内容。

**Gate R5:**

- 两人、两 Node、Codex 执行、Patch/测试回执和人工审查形成完整闭环。
- 只有通过该 Gate，才开始第二 Runtime 评审。

## 9. R6：用第二个 Runtime 验证适配边界

### Task R6.1：先做许可证与产品能力评审

**Files:**

- Add: `rivloom-docs/runtime-reviews/<runtime>-<version>.md`
- Add: `rivloom-docs/adr/0007-select-second-runtime.md`

**Required findings:**

- 固定版本、源码和二进制来源。
- SPDX 许可证、依赖、NOTICE、修改和再分发义务。
- 商业使用、自动化、账号与商标限制。
- 结构化协议、取消、审批、Diff、费用和无头运行能力。
- 结论必须是 `bundle`、`user-provided executable` 或 `do not integrate` 之一。

在这份评审被接受前，不创建 Runtime 适配代码或在 UI 中宣传支持。

### Task R6.2：从两个真实实现提取最小接口

**Files:**

- Modify: `apps/desktop/src-tauri/src/runtime/mod.rs`
- Move only shared types from: `apps/desktop/src-tauri/src/runtime/codex.rs`
- Add: `apps/desktop/src-tauri/src/runtime/<second_runtime>.rs`
- Add: `apps/desktop/src-tauri/src/runtime/<second_runtime>_tests.rs`
- Add: `apps/desktop/src-tauri/src/runtime/contract_tests.rs`
- Add: `apps/desktop/src-tauri/tests/second_runtime_task_run.rs`
- Add: `apps/desktop/src-tauri/tests/support/fake_<second_runtime>.rs`

只抽取两个 Runtime 都真实使用的 capability/auth/start/interrupt/event/artifact 边界。不要
强迫第二 Runtime 模拟 Codex thread，也不要为尚不存在的第三 Runtime 增加扩展点。

**Test first:**

- 第二 Runtime 适配器测试其真实协议映射，包括 capability、auth、start、interrupt、
  event 和 artifact；不能只测试共享 trait 或 DTO。
- `second_runtime_task_run.rs` 使用讲真实协议的测试进程完成与 Codex 相同的
  Task -> Run -> RunReceipt 纵向场景，并覆盖该 Runtime 特有的取消、失败和 Artifact 行为。
- 固定版本的真实 Runtime 还需完成一次人工 smoke，证明自动化测试没有把协议测试替身
  误当成真实产品 Gate。

**Gate R6:**

- 同一个 Task/RunReceipt 协作闭环能在第二 Runtime 上完成。
- Runtime 特有行为没有污染 Brain 协议。

## 10. R7：Skill Directory 的启动条件

只有以下条件同时满足才立项：

1. R5 与 R6 均通过。
2. 至少两个 Runtime 的真实任务都需要发现或分发 Skill。
3. 已回答 Skill 的来源、签名、权限、版本、许可证和恶意内容扫描问题。
4. 能证明 Directory 比用户本地配置或 Runtime 自有能力带来明确协作价值。

不满足时，Skill 继续作为各 Runtime 的本地能力，不进入 Brain 或 Marketplace。

## 11. 每个实现 PR 的固定检查

```powershell
cd apps/desktop
just check
just test-rust
just check-rust
corepack pnpm format
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

cd ..\..\codex-rs
just fmt
```

Rust 依赖变化后，从仓库根按规则运行：

```powershell
just bazel-lock-update
```

然后检查：

```powershell
git diff --check
git status --short
git diff --stat origin/main...HEAD
```

涉及网络、身份、邀请、Artifact 或命令执行的 PR 还必须做有针对性的安全审查。完整测试
套件只在共享 crate 或发布 Gate 需要时运行，并按仓库要求先取得用户确认。
