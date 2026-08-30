# ADR-0007：R2 托管任务临时采用无审批严格沙箱

## 状态

Accepted — Temporary；Windows implementation blocked

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

## 目标决策

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

上述组合是已接受的目标策略，但目前没有在 Windows 产品代码中落地。代码暂时保留
`on-request + auto_review`，R2 真实 success/cancel Gate 保持未通过，直到下述 Windows
沙箱冲突有经过审查的解法。

## Windows 隔离 Home 冲突

`approvalPolicy=never` 只控制 Turn 是否请求升级权限，不负责提供 Windows 原生沙箱。
真实 Gate 得到三组结果：

1. `on-request + auto_review` 会进入上述审批线程栈溢出并收敛为 `outcomeUnknown`。
2. `never` 且没有显式 Windows 沙箱时，本地工具被策略拒绝；Turn 仍可能结束并产生
   0-byte Patch，因此不能把 Turn 完成事件直接当作任务成功。
3. 临时仅对 Rivloom sidecar 追加 `-c windows.sandbox="elevated"` 后，请求的 `cwd`、唯一
   可写根和禁网边界正确，但首个 `exec` 报告沙箱未初始化并触发 Windows 管理员确认。
   测试没有代答系统安全提示，临时代码随后已回滚。

最终复核发现，不能把“为隔离 Rivloom `CODEX_HOME` 完成一次 elevated 初始化”当作安全的
简单前置步骤。对仓库所含上游源码的只读检查显示：

- `codex-rs/windows-sandbox-rs/src/setup.rs` 使用固定的本地 Windows 账号
  `CodexSandboxOffline` 和 `CodexSandboxOnline`；
- `codex-rs/windows-sandbox-rs/src/bin/setup_main/win/sandbox_users.rs` 在初始化时生成新密码，
  并会为这两个固定账号重设密码；
- 新密码只加密保存到发起初始化的 `CODEX_HOME/.sandbox-secrets`。

因此，不同 `CODEX_HOME` 各自执行 elevated 初始化可能重设同一组系统账号密码，使另一个
Home 已保存的凭据失效；两个 Home 可能相互破坏。这个结论是根据源码行为做出的安全推断，
验证过程没有读取任何 `.sandbox-secrets`、账号 Token 或密码。

在问题解决前：

- 不提交 `windows.sandbox="elevated"` 的 Rivloom 进程 override；
- 不要求用户批准隔离 Home 的 elevated 初始化；
- 不复制、链接或共享两个 Home 的 `.sandbox-secrets`；
- 不静默降级到 OpenAI 文档定义为较弱 fallback 的 `unelevated`；
- 不把 `never` 无沙箱时的空 Patch 误报为成功。

## 待决实现路径

必须通过后续独立安全决策明确选择并验证以下一条路径：

1. **临时采用 `unelevated`**：保留隔离 `CODEX_HOME`，接受较弱的 Windows 沙箱，并在产品
   状态、威胁模型和真实 Gate 中明确其网络与身份隔离限制。
2. **保留 `elevated`**：先让 Windows 沙箱账号状态不再由多个 Home 各自持有或相互重置，
   例如等待/推动上游把设备级沙箱状态与 `CODEX_HOME` 分离，或设计经过审查的共享状态
   方案；不能通过复用整个主 Codex Home 来牺牲 Runtime 配置和数据隔离。

这两条路径都会改变安全边界或 Runtime 架构，属于需要用户确认且难以静默回退的决定。

## 恢复审批的条件

本决策不能因为安装了新版本 Runtime 而自动失效。未来恢复
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
- 上游改变 Windows 沙箱账号、凭据或 `CODEX_HOME` 的关系；
- 开始实现本地用户审批 UI，或进入需要 Bob 本地处理审批的 R3/R4 工作；
- 产品明确需要联网、安装依赖或访问受管 worktree 外资源的 Task 能力；
- 上游发布说明或复现结果表明 `codex-approval-review` 的 Windows 故障已修复。

## 结果

### 正面

- 目标策略及其权限边界已经固定，后续实现不会把绕过审批误解为 unrestricted access。
- 真实 Gate 证明 Rivloom 在 Runtime 崩溃、重启和无法核实结果时会 fail closed，不会自动
  重跑或覆盖用户 checkout。
- 双 Home 凭据冲突在提交产品代码前被发现，避免影响主 Codex 客户端的原生沙箱。

### 负面

- R2 的实现、自动化与视觉 Gate 已完成，但真实 success/cancel Gate 仍未收口。
- Windows 上暂时没有同时满足隔离 Home、较强原生沙箱和无人值守执行的已验证组合。
- 进入 R3 前需要一次明确的安全/架构选择，并补做真实 success、cancel 和清理 Gate。

### 中性

- `waitingApproval` 仍属于 Rivloom 的长期任务状态模型，但在目标临时模式中不应由正常 Run
  到达。
- 若越界失败没有足够明确的 Runtime 终态，Rivloom 仍可能使用 `outcomeUnknown`；不得把
  未知结果猜成策略拒绝或成功。

## 考虑过的替代方案

**保持 `on-request + auto_review` 并等待上游修复**

- 当前代码暂时保持该组合以避免提交错误的无沙箱成功路径，但它不是通过 R2 真实 Gate 的
  方案；真实本地任务会稳定断开。

**改用 `approvalsReviewer=user`**

- 暂未采用：Rivloom R2 没有完整的审批请求展示和答复通道，任务可能无限等待。

**放宽为 unrestricted filesystem 或允许网络**

- 拒绝：这会扩大 Codex 的影响范围，不是绕过审批崩溃所必需，也违反 R2 的隔离目标。

**在 Rivloom 中修改或 fork Codex 审批源码**

- 拒绝：违反外部 Runtime 边界，并把上游实现维护成本重新引入 Rivloom。

**审批崩溃后自动降级并重跑**

- 拒绝：原 Run 可能已有副作用，自动重跑会破坏幂等和 `outcomeUnknown` 的安全语义。

**直接复用主 Codex Home**

- 暂未采用：会继承并混合主客户端的配置、技能、会话和其他 Runtime 状态，破坏 Rivloom
  已接受的隔离边界。

## 参考资料

- [ADR-0005：采用外部 Agent Runtime](0005-use-external-agent-runtimes.md)
- [R1/R2 Runtime Host 验证记录](../plans/2026-08-30-runtime-host-r1-r2-verification.md)
- [Runtime Host Transition Implementation Plan](../plans/2026-08-30-runtime-host-transition-plan.md)
- [OpenAI：Windows sandbox](https://learn.chatgpt.com/docs/windows/windows-sandbox)
