# Rivloom Runtime Host R1/R2 验证记录

- 日期：2026-08-30
- 平台：Windows
- 实现基线：`1c531ca9eb`
- Gate 分支：`codex/r2-gate-docs`
- 状态：R1/R2 实现与自动化验证完成；原生宽/窄窗口视觉 Gate 通过；真实 Codex
  success/cancel Gate 被上游 App Server 审批线程崩溃阻塞

## 1. 结论

当前 stacked 分支已经把 Rivloom Desktop 从“项目会话入口”推进为最小 Codex Runtime
Host：Rivloom Identity 与 Codex Runtime Auth 分离；用户可以从已登记项目定义有界任务，
在受管 worktree 中启动、观察和停止一次 Codex Run，并得到可校验 RunReceipt 与 Patch
元数据。主流程不依赖完整 Chat 页面，也没有嵌入或修改 `codex-rs`。

自动化 Gate、原生进程生命周期 smoke 和 Windows 原生 WebView 宽/窄窗口检查均已通过。
真实已登录 Codex 的 Run 也已经使用专用测试仓库启动，但目前不能把 R2 的运行验收写成
“全部通过”：在 `approvalPolicy=on-request`、`approvalsReviewer=auto_review` 下，上游
App Server 的 `codex-approval-review` 线程在 Windows 上稳定栈溢出。即使模型只调用安全的
`Get-Content -LiteralPath gate.txt`，App Server 仍会崩溃并断开。

Rivloom 对该故障按设计 fail closed：Run 进入 `outcomeUnknown`，不会伪报成功或自动重跑；
专用仓库的 checkout、HEAD 和基线文件均未改变。是否临时改用另一种审批策略属于安全与
产品语义决策，本 Gate 没有静默替用户改变。

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
| R2 真实 success/cancel Gate     | 未完成   | 上游 Runtime 缺陷阻塞 | [PR #66](https://github.com/rivloom/rivloom/pull/66)                             |

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
Codex Runtime。真实 turn 明确发送以下执行边界：

- `approvalPolicy=on-request`；
- `approvalsReviewer=auto_review`；
- `sandboxPolicy.type=workspaceWrite`；
- `writableRoots` 只有当前 Run 的受管 worktree；
- `networkAccess=false`；
- Task thread 不注入个人 Skill instructions/catalog。

专用测试仓库证据：

- checkout HEAD 始终为 `24acd311baf599a8f92db9788326df7c12b890bc`；
- `gate.txt` SHA-256 始终为
  `E346803ED953CBD930B6E8B5B5489F625A45347CC7301B512D4DFC561C781616`；
- `git status --porcelain=v1` 为空；
- 没有残留受管 worktree，也没有提交或覆盖用户 checkout。

真实 Run 已到达模型工具调用。最小只读命令 `Get-Content -LiteralPath gate.txt` 也会使上游
App Server 的 `codex-approval-review` 线程栈溢出，随后连接中断。Rivloom 将结果持久化为
`outcomeUnknown`，回执 Patch 为 0 bytes，且不会自动重跑。因此：

- success、Patch 正文与成功 RunReceipt Gate：未通过，被同一上游缺陷阻塞；
- cancel Gate：未能在 Runtime 保持存活的执行窗口内可靠完成，被同一缺陷阻塞；
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

- Task Store v1 实测为 10 个 Task、9 个回执；回执 Patch 只含 `baselineCommit`、`state`、
  `limitBytes`、`byteCount`、`sha256` 五个字段。
- Task Store 实测不含 Windows 绝对路径、Token 字段、`CODEX_HOME`/`codex-home` 或 Patch
  `body`/`content` 字段。
- Task Store 只持久化不透明 project ID，不保存本机绝对项目路径。
- Runtime Token、完整环境变量和原始 App Server payload 不属于 Task、RunReceipt 或
  前端 event contract。
- Rust 命令只暴露固定 task list/start/stop/Patch 读取面，错误映射为封闭枚举。
- worktree 创建和清理都校验受管根目录；不覆盖用户 checkout，也不递归删除未验证路径。
- Patch 正文只作为当前进程内的短期 Artifact 提供给本机 UI，不进入 RunReceipt 或 Task Store。
- 本次验证没有读取、记录或提交账号 Token、OAuth URL 或账号文件内容。

## 9. 阻塞决策与下一步

1. 保持当前 `on-request + auto_review` 策略并等待/升级到修复该 Windows 栈溢出的上游
   Runtime；或者单独评审改为 `never`，或补齐 `user` reviewer 的本机审批交互。三者会改变
   安全与无人值守语义，不能作为 Gate 内部修补静默选择。
2. 上述策略确定且 Runtime 可稳定执行后，在同一专用仓库重跑 success、Patch、RunReceipt、
   cancel 和 worktree cleanup Gate。
3. 按 [stacked PR queue](2026-08-30-runtime-host-pr-stack.md) 从 #41 开始依次审查和合并；
   前一项合并后再把下一项 base 改回 `main`。
4. Runtime Gate 补齐且 R1/R2 stack 审查合并后进入 R3.1；不提前开发第二 Runtime、
   Marketplace 或 Skill Directory。

CI 继续保持暂停；本记录不以缺少 CI 失败邮件代替任何本地测试证据。
