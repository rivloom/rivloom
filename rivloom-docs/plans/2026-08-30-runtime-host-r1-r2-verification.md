# Rivloom Runtime Host R1/R2 验证记录

- 日期：2026-08-30
- 决策更新：2026-08-31，用户确认保留 elevated 方向，并接受 R2 带已知限制收口
- 平台：Windows
- 实现基线：`1c531ca9eb`
- Gate 分支：`codex/r2-gate-docs`
- 状态：R2 实现里程碑已接受收口；自动化与原生宽/窄窗口视觉 Gate 通过；真实 Windows
  success/cancel 验收以后续项 `R2-FU1` 保留

## 1. 结论

当前 stacked 分支已经把 Rivloom Desktop 从“项目会话入口”推进为最小 Codex Runtime
Host：Rivloom Identity 与 Codex Runtime Auth 分离；用户可以从已登记项目定义有界任务，
在受管 worktree 中启动、观察和停止一次 Codex Run，并得到可校验 RunReceipt 与 Patch
元数据。主流程不依赖完整 Chat 页面，也没有嵌入或修改 `codex-rs`。

自动化 Gate、原生进程生命周期 smoke 和 Windows 原生 WebView 宽/窄窗口检查均已通过。
真实已登录 Codex 的 Run 也已经使用专用测试仓库启动，但目前不能把 R2 的运行验收写成
“全部通过”。当前 `approvalPolicy=on-request`、`approvalsReviewer=auto_review` 会让上游
App Server 的 `codex-approval-review` 线程在 Windows 上稳定栈溢出。临时改为
`approvalPolicy=never` 后该崩溃路径不再出现，但隔离 Rivloom `CODEX_HOME` 没有继承主 Codex
Home 的 Windows 沙箱设置和身份；没有显式选择原生沙箱时，模型工具被策略拒绝，Turn 的
完成事件也不能证明任务目标已经完成。

临时为 sidecar 选择 `windows.sandbox="elevated"` 后，请求/rollout 声明的 worktree 和禁网
参数正确，但首次执行需要初始化，尚未证明 OS 隔离已生效。源码复核发现 elevated 使用固定
系统账号，而账号密码分别保存在各自 `CODEX_HOME`：为隔离 Home 初始化可能重设主 Codex
Home 使用的同一账号密码。该风险不能靠代答一次管理员确认来解决。因此临时产品代码已
回滚，当前代码继续保持
`on-request + auto_review`；[ADR-0007](../adr/0007-temporarily-disable-managed-run-approvals.md)
记录已接受的目标策略及已选定的 elevated 方向，不宣称 Windows 实施已经完成。

Rivloom 对已验证的 Runtime 崩溃和执行中重启按设计 fail closed：Run 进入 `outcomeUnknown`，
不会自动重跑。`never` 工具受拒但 Turn 完成的诊断场景不能算目标成功，不能笼统归为同一种
未知终态。专用仓库的 checkout、HEAD 和基线文件均未改变。用户已选择 elevated，并把
共存接入及真实 success/cancel 验收后移为 `R2-FU1`，不再阻塞 R2 收口和 R3 开工。

## 2. 里程碑完成度

| 范围                             | 本地实现 | 验证状态             | 发布状态                                                                        |
| -------------------------------- | -------- | -------------------- | ------------------------------------------------------------------------------- |
| R1.1 身份与 Runtime Auth 契约    | 完成     | 通过                 | [PR #41](https://github.com/rivloom/rivloom/pull/41)                             |
| R1.2 本地身份存储                | 完成     | 通过                 | [PR #42](https://github.com/rivloom/rivloom/pull/42)                             |
| R1.3 双状态首页                  | 完成     | 通过                 | [PR #44](https://github.com/rivloom/rivloom/pull/44)                             |
| R2.1 Task/Run 状态机             | 完成     | 通过                 | [PR #45](https://github.com/rivloom/rivloom/pull/45)                             |
| R2.2 版本化 Task Store           | 完成     | 通过                 | [PR #46](https://github.com/rivloom/rivloom/pull/46)                             |
| R2.3 Codex Run 事件路由          | 完成     | 通过                 | [PR #47](https://github.com/rivloom/rivloom/pull/47)、[#48](https://github.com/rivloom/rivloom/pull/48) |
| R2.4 worktree 与 Patch Artifact  | 完成     | 通过                 | [PR #49](https://github.com/rivloom/rivloom/pull/49)、[#50](https://github.com/rivloom/rivloom/pull/50) |
| R2.5 RunReceipt、编排、命令与 UI | 完成     | 自动化通过           | [PR #51](https://github.com/rivloom/rivloom/pull/51) 到 [#64](https://github.com/rivloom/rivloom/pull/64) |
| R2 原生视觉 Gate                | 完成     | 通过                 | [PR #66](https://github.com/rivloom/rivloom/pull/66)                             |
| R2 里程碑收口                   | 已接受   | 带已知 Windows Runtime 限制 | [ADR-0008](../adr/0008-close-r2-with-deferred-windows-runtime-validation.md) |
| R2-FU1 真实 success/cancel Gate | 后续项   | elevated 共存接入待验证 | [PR #66](https://github.com/rivloom/rivloom/pull/66)                         |

R3 及之后没有提前开始。当前代码仍是 Codex 专用路径，没有为了 Hermes、Reasonix、
Claude Code 或未知第三 Runtime 创建万能适配器。

## 3. 实现审查单元

R1：

- `bb3502cfb8`：Identity 与 Runtime Auth wire contract。
- `6a98c765b8`：版本化本地 Identity 存储。
- `f8a30988b7`：Identity 与 Codex Runtime 双状态 UI。

R2 基础：

- `b49c658e90`：有界 Task/Run 状态机。
- `a01e311cff`：版本化、幂等 Task Store。
- `c429229840`、`329801e0b9`：有界事件路由与 Codex Run。
- `43c71df34b`、`f02c9bb95d`：受管 worktree 与 Patch Artifact。
- `40e06f67b0`、`825f466fe3`、`460af172c7`：RunReceipt 与幂等持久化；Patch 正文不进入回执。

R2 纵向闭环：

- `66b1054d29`、`0aca7684a8`、`a40705cfaf`：Run claim、隔离执行与 App Server 事件接线。
- `ab47b214a2`、`14ede68c5f`、`282b4c2802`、`74a561db56`：项目绑定、重启对账、worker fail-closed 与监管。
- `74a902bd93`、`f2614cf9f2`：固定 Tauri 命令和 React Bridge/Hook。
- `f419cb2867`、`53e5ad0d3d`、`0e11e64b6e`：有界 Composer、Run 文案与回执 UI。
- `9c93177633`、`1c531ca9eb`：项目工作区主流程切换为 Task，并清理旧 Chat affordance。

每个非机械审查单元都保持在 800 changed lines 内；较大的 R2.5 被拆成可独立回滚的
纵向步骤，而不是一个总 PR。

## 4. 自动化证据

在 `apps/desktop` 对 Gate 分支运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib --features test-tauri-commands task::commands
cargo clippy --manifest-path src-tauri/Cargo.toml --tests -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --lib --features test-tauri-commands -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
pnpm test
pnpm run build
```

结果：

- Rust：211 项通过，0 失败；doc test 与 binary test 无失败。
- Feature-gated Tauri task command：1 项通过，证明命令已注册且只返回脱敏错误。
- 普通与 feature-gated Clippy：均以 `-D warnings` 通过。
- Rustfmt：通过。
- React/Vitest：21 个测试文件、92 项测试通过。
- TypeScript project build 与 Vite production build：通过。

Rust 测试覆盖真实临时 Git 仓库与 worktree 操作；App Server 使用受控测试连接，不调用
真实模型。测试构建只创建 Git 忽略的 sidecar hardlink 和本地构建输出，没有提交二进制。

## 5. Windows 原生 Runtime Gate

使用仓库中已有 sidecar 启动 `tauri dev`，并连接隔离 Rivloom `codex-home` 中已登录的真实
Codex Runtime。当前产品请求和两组临时 Gate 请求均保持以下执行边界：

- `sandboxPolicy.type=workspaceWrite`；
- `writableRoots` 只有当前 Run 的受管 worktree；
- `networkAccess=false`；
- Task thread 不注入个人 Skill instructions/catalog。

审批/Windows 沙箱组合分别是：当前产品代码的 `on-request + auto_review`；临时 Gate 的
`never`；以及临时 Gate 的 `never` 加 sidecar 进程级
`-c windows.sandbox="elevated"`。后两组只用于诊断，均已从工作区代码回滚。

本次失败 sidecar 报告版本为 `codex-app-server 0.0.0`，不足以唯一定位构建；已补充记录二进制
SHA-256：`4F57C510209BE79AF617FF261A7293F71AD5B9D66411386C3D9DCC5A2D5C97FD`。
OpenAI 的 Windows 沙箱文档将 `elevated` 定义为首选的较强实现，并说明它需要管理员批准的
本机设置；`unelevated` 是较弱 fallback。本 Gate 没有自动降级，没有读取或复制另一个
Codex Home 的 `.sandbox-secrets`，也没有批准 Windows 安全提示。

专用测试仓库证据：

- checkout HEAD 始终为 `24acd311baf599a8f92db9788326df7c12b890bc`；
- `gate.txt` SHA-256 始终为
  `E346803ED953CBD930B6E8B5B5489F625A45347CC7301B512D4DFC561C781616`；
- `git status --porcelain=v1` 为空；
- 最新管理员确认阻塞尝试保留了一个未修改的诊断 worktree；没有提交或覆盖用户 checkout。

真实 Gate 分三步得到以下证据：

1. `on-request + auto_review` 基线：最小只读命令也会使 `codex-approval-review` 线程栈溢出，
   Rivloom 按 `outcomeUnknown` fail closed。
2. `never` 且未显式选择 Windows 沙箱：审批线程不再崩溃，但本地工具被策略拒绝；Turn 完成、
   Patch 为 0 bytes。该结果不算成功，也暴露了“Turn completed 不等于目标已完成”的验收边界。
3. `never + elevated` 进程 override：Rollout 记录的 `cwd`、唯一可写根和禁网均正确；首个
   `exec` 到达 Windows 沙箱，日志明确写入
   `sandbox setup required: sandbox setup marker missing or incompatible`，随后出现管理员确认。
   测试在未代答系统安全提示的前提下停止，受管文件未改变。客户端重启后，该 Run 按设计
   对账为 `outcomeUnknown`，错误为“重启前无法核实结果”，且没有自动重跑。

管理员确认不是唯一剩余步骤。对仓库所含上游源码的只读复核发现：Windows elevated 沙箱
使用固定的 `CodexSandboxOffline`/`CodexSandboxOnline` 系统账号；初始化会生成并重设这两个
账号的密码，但只把新凭据保存到发起初始化的 `CODEX_HOME/.sandbox-secrets`。由此推断，
隔离 Home 的初始化可能使主 Codex Home 已保存的凭据失效。这个多 Home 冲突发生在设备级
账号和 Home 级状态的边界，不能通过继续点击一次 UAC 安全解决。

因此：

- 原审批栈溢出回归：`never` 路径未复现，诊断通过；
- success、Patch 正文与成功 RunReceipt Gate：未通过，等待 elevated 共存接入与验证；
- cancel Gate：未完成；必须在可用的正常执行窗口内用产品 Stop 流程复验；
- fail-closed、断连对账、原 checkout 不变：真实进程证据通过。

## 6. Windows 原生视觉 Gate

- 宽窗口 `1182 × 792`：项目任务表单、任务列表、状态时间线、RunReceipt 与 Patch 元数据
  均可阅读，无横向溢出、重叠或被侧栏遮挡。
- 窄窗口约 `962 × 672`：这是当前原生窗口允许的约 960px 宽窄布局和最小高度；表单、
  双栏事件/回执区和长中英文任务标题会正常换行，没有横向滚动条或内容截断。
- 连接状态、结果未知警示和 0-byte Patch 元数据在原生 WebView 中与 snapshot 语义一致。

视觉 Gate 只验证布局和可读性，不把 `outcomeUnknown` 误算为真实执行成功。

## 7. Gate R2 行为证据

- Task、Run、event、summary、error、测试项和最终 prompt 都有硬上限。
- `turn/start` 的 `cwd` 只能来自后端已登记项目生成的受管 worktree，WebView 不能传入路径。
- 同一幂等请求只 claim 和执行一次；重启不会自动重跑可能已有副作用的 Run。
- App Server 断开、worker 丢失或启动失败进入 `outcomeUnknown`，不会显示虚假成功。
- stop 必须同时匹配 Task ID、Run ID、Runtime thread/turn 与当前连接。
- success、failed、cancelled、`outcomeUnknown` 都生成或保留语义明确的终态。
- RunReceipt 绑定 Task、Run、Runtime 版本、时间、终态、测试摘要、Patch baseline、大小与
  SHA-256；同一回执幂等，不同回执不能覆盖。
- UI 从现有项目页直接创建 Task，只展示最近 6 条有界事件、审批等待、测试摘要和回执。
- Runtime 未报告测试时显示“未报告”，不把缺失信息猜成通过。
- Patch 正文按需读取，初始 DOM 和 Task Store 都不包含正文；超限 Patch 只显示元数据。

## 8. 安全与隐私审计

- Task Store v1 在最新 Gate 尝试后实测为 15 个 Task、11 个回执；回执 Patch 只含
  `baselineCommit`、`state`、`limitBytes`、`byteCount`、`sha256` 五个字段。
- Task Store 实测不含 Windows 绝对路径、Token 字段、`CODEX_HOME`/`codex-home` 或 Patch
  `body`/`content` 字段。
- Task Store 只持久化不透明 project ID，不保存本机绝对项目路径。
- Runtime Token、完整环境变量和原始 App Server payload 不属于 Task、RunReceipt 或
  前端 event contract。
- Rust 命令只暴露固定 task list/start/stop/Patch 读取面，错误映射为封闭枚举。
- worktree 创建和清理都校验受管根目录；不覆盖用户 checkout，也不递归删除未验证路径。
- Patch 正文只作为当前进程内的短期 Artifact 提供给本机 UI，不进入 RunReceipt 或 Task Store。
- 本次验证没有读取、记录或提交账号 Token、OAuth URL 或账号文件内容。

## 9. R2 收口决定与下一步

根据 [ADR-0008](../adr/0008-close-r2-with-deferred-windows-runtime-validation.md)，R2 作为
实现里程碑已接受收口。真实 Runtime 未通过项保留为 `R2-FU1`；后续安排如下：

1. 开始 R3.1；`R2-FU1` 不阻塞 R3 实现，但必须在接受 Gate R4 和对外宣称 Windows 本地任务
   闭环可用前完成。不提前开发第二 Runtime、Marketplace 或 Skill Directory。
2. 按 [stacked PR queue](2026-08-30-runtime-host-pr-stack.md) 从 #41 开始依次审查和合并；
   前一项合并后再把下一项 base 改回 `main`。R3.1 可以在该 stack 继续审查期间开始。
3. 处理 `R2-FU1` 时保留 elevated，执行 ADR-0007 的候选版本、独立测试环境和双 Home
   共存 Gate。当前所查配置和公开 setup API 没有账号名/凭据目录 override；这不是等待用户
   选择，而是已选路线的接入阻塞。不复用整个主 Codex Home，不复制或链接 `.sandbox-secrets`。
4. 有受支持且通过共存验证的方案后，在独立、有界的实现 PR 中落地，再重跑 success、Patch、
   RunReceipt、cancel 和 worktree cleanup Gate，并明确记录越界拒绝的终态。不得只凭 Turn
   completed 或正确的请求参数认定目标/沙箱验收通过，不得自动扩大权限或重跑未知结果。
5. 未来恢复 `on-request + auto_review` 前必须满足 ADR-0007 的版本固定、原始回归、真实
   审批、安全审查和独立 PR 条件；不得随 Runtime 升级静默切换。

CI 继续保持暂停；本记录不以缺少 CI 失败邮件代替任何本地测试证据。
