# Rivloom Runtime Host R1/R2 验证记录

- 日期：2026-08-30
- 平台：Windows
- 验证提交：`1c531ca9eb`
- 状态：R1、R2 本地实现、自动化验证与原生进程 smoke 完成；PR 队列和真实 Run smoke 待完成

## 1. 结论

当前 stacked 分支已经把 Rivloom Desktop 从“项目会话入口”推进为最小 Codex Runtime
Host：Rivloom Identity 与 Codex Runtime Auth 分离；用户可以从已登记项目定义有界任务，
在受管 worktree 中启动、观察和停止一次 Codex Run，并得到可校验 RunReceipt 与 Patch
元数据。主流程不依赖完整 Chat 页面，也没有嵌入或修改 `codex-rs`。

自动化 Gate 与不调用模型的原生进程 smoke 已通过，但还不能把 R2 写成“已发布并完成人工
验收”：R1.2 到 R2.5a 的远端分支尚未创建 PR，R2.5b 之后仍只在本地；真实已登录 Codex
的 Run smoke 也没有在无人值守状态下调用。后者会使用真实账号和模型，并可能等待本机
审批，应在发布前由用户知情参与一次。

## 2. 里程碑完成度

| 范围                             | 本地实现 | 自动化验证 | 发布状态                       |
| -------------------------------- | -------- | ---------- | ------------------------------ |
| R1.1 身份与 Runtime Auth 契约    | 完成     | 通过       | PR #41 已创建                  |
| R1.2 本地身份存储                | 完成     | 通过       | 远端分支已上传，PR 待创建      |
| R1.3 双状态首页                  | 完成     | 通过       | 远端分支已上传，PR 待创建      |
| R2.1 Task/Run 状态机             | 完成     | 通过       | 本地与远端分支均有对应审查单元 |
| R2.2 版本化 Task Store           | 完成     | 通过       | 本地与远端分支均有对应审查单元 |
| R2.3 Codex Run 事件路由          | 完成     | 通过       | 本地与远端分支均有对应审查单元 |
| R2.4 worktree 与 Patch Artifact  | 完成     | 通过       | 本地与远端分支均有对应审查单元 |
| R2.5 RunReceipt、编排、命令与 UI | 完成     | 通过       | R2.5a 已上传；R2.5b 之后仅本地 |

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

在 `apps/desktop` 对验证提交运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib --features test-tauri-commands task::commands
cargo clippy --manifest-path src-tauri/Cargo.toml --tests -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --lib --features test-tauri-commands -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
vitest run
tsc -b --pretty false
vite build
```

结果：

- Rust：211 项通过，0 失败；doc test 与 binary test 无失败。
- Feature-gated Tauri task command：1 项通过，证明命令已注册且只返回脱敏错误。
- 普通与 feature-gated Clippy：均以 `-D warnings` 通过。
- Rustfmt：通过。
- React/Vitest：21 个测试文件、92 项测试通过。
- TypeScript project build 与 Vite production build：通过。
- `git diff --check`：通过；Windows checkout 仅有预期的 LF/CRLF 提示。

Rust 测试覆盖真实临时 Git 仓库与 worktree 操作；App Server 使用受控测试连接，不调用
真实模型。测试构建只创建了 Git 忽略的 sidecar hardlink 和本地构建输出，没有提交二进制。

## 5. Windows 原生进程 smoke

使用仓库中已有 sidecar 运行 `tauri dev`，结果如下：

- Tauri/Rust 开发构建完成并启动 `rivloom-desktop.exe`。
- App Server 状态按 `stopped -> starting -> connected` 转换，握手返回 Windows 平台和隔离的
  Rivloom `codex-home`；没有使用官方 Codex Desktop 的用户数据目录。
- 当前隔离账号未登录，featured plugin 请求出现 401/429 警告，但没有阻止 Runtime 连接，
  也没有触发 `thread/start`、`turn/start` 或模型请求。
- 尝试自动读取原生窗口布局时，Windows 桌面控制授权因用户不在而超时；没有绕过授权，
  没有点击登录、项目或任务操作。进程随后停止。

因此原生构建与 Runtime 生命周期已经 smoke 通过；窄/宽窗口的最终视觉检查和真实 Task
Run 仍明确保留为人工验收，不能由这次启动结果替代。

## 6. Gate R2 行为证据

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

## 7. 安全与隐私审计

- Task Store 只持久化不透明 project ID，不保存本机绝对项目路径。
- Runtime Token、`CODEX_HOME`、完整环境变量和原始 App Server payload 不属于 Task、
  RunReceipt 或前端 event contract。
- Rust 命令只暴露固定 task list/start/stop/Patch 读取面，错误映射为封闭枚举。
- worktree 创建和清理都校验受管根目录；不覆盖用户 checkout，也不递归删除未验证路径。
- Patch 正文只作为当前进程内的短期 Artifact 提供给本机 UI，不进入 RunReceipt 或 Task Store。
- 审批仍留在执行 Node；当前 UI 只显示等待状态，不提供远端代批能力。
- 本次验证没有读取、记录或提交账号 Token、OAuth URL、账号文件或用户工作区内容。

## 8. 尚未完成与下一步

1. 为已上传的 R1.2、R1.3、R2.1 到 R2.5a 分支按 stack 顺序创建 PR；R1.1 当前为
   [PR #41](https://github.com/rivloom/rivloom/pull/41)。R2.5b 之后需先得到明确的远程分支
   上传许可。本地完成不冒充远端已发布。
2. 在用户知情参与时，用已登录的 Codex Runtime 和一个专用测试仓库完成原生 smoke：
   启动任务、观察状态、按需处理本机审批、停止一次 Run、验证 Patch 与 RunReceipt，并确认
   用户 checkout 未改变。
3. 做一次真实 Tauri 窗口的窄/宽布局检查；自动化 snapshot 已覆盖文案和交互，但不替代
   Windows 原生 WebView 的最终视觉验收。
4. 上述两项完成并发布后进入 R3.1；不提前开发第二 Runtime、Marketplace 或 Skill Directory。

CI 继续保持暂停；本记录不以缺少 CI 失败邮件代替任何本地测试证据。
