# Rivloom Runtime Host R3.1 验证记录

日期：2026-08-31。状态：协议 v1 实现与本地自动化验证完成；PR 待审查合并，Gate R3 未通过。

## 基线与审查单元

- 先核对本地 R1/R2 合并收尾记录、远端主干计划、验证记录、PR queue 和 ADR-0007/0008。
  收尾记录实际位于 `C:/project/opencohive/.worktrees/2026-08-31-r1-r2-merge-closeout.md`，
  而非交接消息中的同名兄弟目录。
- `git fetch origin main` 后从 `8140f7c46b` 创建独立 worktree
  `C:/project/opencohive/.worktrees/r3-1-collaboration-protocol`。
  主目录仍在旧本地 main，其原有 `apps/desktop/src-tauri/Cargo.toml` 改动未覆盖。
- R3.1a：[Draft PR #67](https://github.com/rivloom/rivloom/pull/67)，
  `codex/r3-1-collaboration-protocol` → `main`，提交 `f2536f4abf`，579 changed lines。
- R3.1b：`codex/r3-1-receipt-contract` → `codex/r3-1-collaboration-protocol`，
  补齐回执/Artifact 和严格字符串枚举。先合并 #67，再重定此 PR 的 base 为 main。
  两个单元分别验证，每个 PR 均低于 800 changed lines；没有 force-push 或自动合并。

## 实现与行为证据

协议说明见 [Collaboration protocol v1](collaboration-protocol-v1.md)。

- 六种消息有显式版本、消息 ID、幂等键、Brain/发送 Node、Unix 秒和修订号。
- 接收在解析前限制 32 KiB，发送验证最终 JSON 编码大小；UTF-8 字段、集合数量和总量均有上限。
- 未知版本、未知/重复字段、对象形式的字符串枚举别名、路径型 ID 和权限扩展字段被拒绝。
- Assignment 只记录本地接受关联；协议解析不创建 worktree、不批准权限、不启动 Runtime。
- 共享回执保留成功、失败、取消与结果未知；测试未报告不冒充通过。
- Patch 只传有界元数据；校验 commit/hash 格式、状态与大小一致性，回执绑定 Task/Run，
  按固定序列化顺序重算内容 SHA-256；篡改和内部关联不一致均被拒绝。
- 对外错误固定，不回显输入；golden payload 没有路径、Token、日志、环境变量或 App Server 消息。
  文本可能由用户填入敏感内容，schema 不提供 DLP，分享确认仍须在后续产品接线时实现。

初始宽松实现使 6 项新增测试失败，随后修复；额外回归复现了 Serde 无字段枚举分支忽略
额外字段、以及字符串枚举接受对象别名两类问题，均已修复。变更回执字段的测试先重算合法
hash，再检验语义校验，避免仅因旧 hash 不匹配而假通过。生产模块 440 行，无新增依赖。

## 自动化结果

在上述独立 worktree 的 `apps/desktop` 执行，R3.1a 与 R3.1b 均单独通过：

| 检查 | R3.1a | R3.1b 最终 |
| --- | --- | --- |
| `just test-rust` | 219 项 + 4 项 feature-gated 命令 | 224 项 + 4 项 feature-gated 命令 |
| `just check` | 21 文件 / 95 项；TS/Vite build 通过 | 21 文件 / 95 项；TS/Vite build 通过 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --tests -- -D warnings` | 通过 | 通过 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --lib --features test-tauri-commands -- -D warnings` | 通过 | 通过 |
| 桌面 `cargo fmt` 与 `--check` | 通过 | 通过 |
| `git diff --check`、文档本地链接、受保护范围检查 | 通过 | 通过 |

测试复用本机依赖、Cargo 缓存和忽略的 sidecar hardlink，没有调用真实模型。
前端快照只有 Windows 换行状态刷新，无语义变化；未提交 UI 或快照改动。
测试与 Clippy 先完成，最后格式化；未在格式化后重复运行同一批测试。
在 `codex-rs` 执行仓库级 `just fmt` 仍因既有失效 Python 启动器失败；
没有修改全局 Python 配置或上游源码，该项不记为通过。

## 安全审查与未完成 Gate

本轮审查确认：没有开放端口或网络客户端，没有 Runtime 调用或秘密读取，没有扩大权限，
没有对 R2 本地存储/回执做不兼容迁移。认证、角色校验、修订号权威、重放冲突处理和邀请
不是 R3.1 已实现能力；数据里的角色/接受者/哈希不能自行证明这些权限或事实。

`R2-FU1` 的 Windows elevated 多 Home 共存，以及真实 success、Patch、RunReceipt、
cancel、越界拒绝和 cleanup 验收仍延期；Gate R4 和 Windows 可用性发布前必须补齐。
当前产品继续 `on-request + auto_review`；未启用 `never + elevated` 或降低沙箱边界。
未修改 `codex-rs`、CI、旧 Draft PR #37/#38；未使用子 agent，未引入第二 Runtime、
Marketplace 或 Skill Directory。

下一步：审查并顺序合并两张 R3.1 PR，再开始 R3.2 邀请、成员与 Node 凭证。
R3.3 Brain 状态存储、R3.4 连接/对账及两机 Gate R3 仍未实现/验收。
