# ADR-0007：R2 托管任务临时采用无审批严格沙箱

## 状态

Accepted — Temporary

## 背景

R2 通过外部 `codex-app-server` 在受管 Git worktree 中执行本地 Codex Task。Rivloom 当前
为每次 `turn/start` 发送 `approvalPolicy=on-request`、
`approvalsReviewer=auto_review`、`sandboxPolicy.type=workspaceWrite`，并把可写根限制为
当前 Run 的受管 worktree，同时关闭网络访问。

2026-08-30 的 Windows 原生 Gate 使用已登录 Runtime 运行真实任务时，App Server 的
`codex-approval-review` 线程会稳定栈溢出。即使模型只请求在受管 worktree 内执行
`Get-Content -LiteralPath gate.txt`，App Server 仍会断开，Rivloom 只能按设计把 Run
收敛为 `outcomeUnknown`。本次失败二进制报告 `codex-app-server 0.0.0`，因此使用
SHA-256 `4F57C510209BE79AF617FF261A7293F71AD5B9D66411386C3D9DCC5A2D5C97FD`
作为可复现指纹。

自动审批属于 Codex Runtime，不是 Rivloom 实现的审批器。Rivloom R2 也没有向本地用户
展示并回答 App Server 审批请求的完整交互。继续等待上游修复会阻塞 R2 真实
success/cancel Gate；放宽沙箱、启用网络或修改上游 Codex 源码都会扩大当前里程碑范围和
风险。

## 决策

- 仅对 Rivloom 创建的本地受管 Codex Task Run，把 `turn/start.approvalPolicy` 临时设为
  `never`，并省略在该策略下无效的 `approvalsReviewer`。
- 保持 `sandboxPolicy.type=workspaceWrite`；`writableRoots` 必须且只能包含当前 Run 的受管
  worktree，`networkAccess` 必须为 `false`，`cwd` 必须与该 worktree 一致。
- 该策略按 Run 通过 App Server 请求发送，不修改用户的 Codex 全局配置，不影响手动
  Codex Task、ChatGPT 登录或 Runtime Auth。
- 需要联网、写入 worktree 外、修改受保护路径或以其他方式越界的操作不能请求临时例外；
  Rivloom 不自动扩大权限，也不以不同策略自动重跑。无法证明结果时继续 fail closed。
- R2 不提供该策略的用户可见开关。未来的人类审批 UI 和远端 Node 本地审批语义必须另行
  设计和验收，不能通过隐藏设置提前引入。
- 实现只修改 Rivloom 的 Codex Run 请求及对应测试，不修改或 fork `codex-rs`。
- 协议和 UI 中已有的 `waitingApproval` 状态保留给未来恢复审批后的流程；它不构成 R2
  严格无人值守模式已经支持审批的声明。

## 恢复条件

本决策不能因为安装了新版本 Runtime 而自动失效。恢复
`approvalPolicy=on-request` 与 `approvalsReviewer=auto_review` 必须同时满足：

1. 固定并记录候选 Codex App Server 的可识别版本；若版本仍不可识别，则至少记录发行来源
   和二进制 SHA-256。
2. 在 Windows 上重跑原始回归：受管 worktree 内的安全读写和测试不得进入崩溃的审批
   路径，App Server 在 Turn 完成后仍保持可用。
3. 使用专用测试仓库触发一次确实需要审批的边界请求，证明自动审查能给出有界且确定的
   允许或拒绝结果，并且不会写入允许范围外或启用网络。
4. 真实 success、Patch、RunReceipt、cancel 和 worktree cleanup Gate 全部通过；用户
   checkout、HEAD 和基线文件保持不变。
5. 请求 payload 测试锁定恢复后的审批与沙箱组合，并完成一次安全审查。
6. 通过独立 PR 更新本 ADR 状态为 `Superseded by ADR-XXXX`。不得只改一行配置而不保留
   决策和验证记录。

## 复核触发器

以下任一事件发生时必须重新评审本决策，但不能未经上述 Gate 自动恢复审批：

- 升级或替换 Rivloom 捆绑的 Codex App Server；
- 开始实现本地用户审批 UI，或进入需要 Bob 本地处理审批的 R3/R4 工作；
- 产品明确需要联网、安装依赖或访问受管 worktree 外资源的 Task 能力；
- 上游发布说明或复现结果表明 `codex-approval-review` 的 Windows 故障已修复。

## 结果

### 正面

- 绕过当前会崩溃的自动审批路径，使 R2 能在边界明确的环境中完成真实 Gate。
- 权限边界仍由 worktree 沙箱和禁网强制执行；`never` 不等于 unrestricted access。
- 任务无需等待 Rivloom 当前尚未实现的审批 UI，适合无人值守本地执行。
- 策略按 Run 生效，回退不需要迁移用户的 Codex 全局配置或认证数据。

### 负面

- 需要下载依赖、访问网络、写入受管 worktree 外或修改 Git 管理数据的任务不能完成。
- Runtime 无法在执行中请求一次窄范围例外；用户只能修改任务或等待产品恢复审批能力。
- 该 Gate 只能证明严格无人值守模式可用，不能证明自动审批或本地人工审批体验可用。

### 中性

- `waitingApproval` 仍属于 Rivloom 的长期任务状态模型，但在本临时模式中不应由正常 Run
  到达。
- 若越界失败没有足够明确的 Runtime 终态，Rivloom 仍可能使用 `outcomeUnknown`；不得把
  未知结果猜成策略拒绝或成功。

## 考虑过的替代方案

**保持 `on-request + auto_review` 并等待上游修复**

- 未采用为当前 R2 Gate 策略：真实本地任务会稳定断开，无法完成端到端验收。

**改用 `approvalsReviewer=user`**

- 暂未采用：Rivloom R2 没有完整的审批请求展示和答复通道，任务可能无限等待。

**放宽为 unrestricted filesystem 或允许网络**

- 拒绝：这会扩大 Codex 的影响范围，不是绕过审批崩溃所必需，也违反 R2 的隔离目标。

**在 Rivloom 中修改或 fork Codex 审批源码**

- 拒绝：违反外部 Runtime 边界，并把上游实现维护成本重新引入 Rivloom。

**审批崩溃后自动降级并重跑**

- 拒绝：原 Run 可能已有副作用，自动重跑会破坏幂等和 `outcomeUnknown` 的安全语义。

## 参考资料

- [ADR-0005：采用外部 Agent Runtime](0005-use-external-agent-runtimes.md)
- [R1/R2 Runtime Host 验证记录](../plans/2026-08-30-runtime-host-r1-r2-verification.md)
- [Runtime Host Transition Implementation Plan](../plans/2026-08-30-runtime-host-transition-plan.md)
