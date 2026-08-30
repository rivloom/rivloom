# Rivloom Runtime Host R1/R2 stacked PR queue

- 日期：2026-08-30
- 已创建：[PR #41](https://github.com/rivloom/rivloom/pull/41)
- CI：按当前决定保持暂停

## 已上传、可创建 PR

以下每一行的 base 都是上一行的 head；创建前仍应检查同一 head 没有现有 PR，避免重复。

| 顺序  | Base                          | Head                        | Changed lines | 建议标题                                                |
| ----- | ----------------------------- | --------------------------- | ------------: | ------------------------------------------------------- |
| R1.2  | `codex/r1-identity-contracts` | `codex/r1-identity-storage` |           784 | `feat(desktop): persist local identity`                 |
| R1.3  | `codex/r1-identity-storage`   | `codex/r1-identity-ui`      |           786 | `feat(desktop): distinguish identity from runtime auth` |
| R2.1  | `codex/r1-identity-ui`        | `codex/r2-task-run-state`   |           718 | `feat(desktop): add bounded task run state machine`     |
| R2.2  | `codex/r2-task-run-state`     | `codex/r2-task-store`       |           720 | `feat(desktop): persist bounded tasks idempotently`     |
| R2.3a | `codex/r2-task-store`         | `codex/r2-event-router`     |           653 | `feat(desktop): route bounded Codex run events`         |
| R2.3b | `codex/r2-event-router`       | `codex/r2-codex-run`        |           501 | `feat(desktop): start and interrupt bounded Codex runs` |
| R2.4a | `codex/r2-codex-run`          | `codex/r2-worktree`         |           624 | `feat(desktop): isolate task runs in managed worktrees` |
| R2.4b | `codex/r2-worktree`           | `codex/r2-patch-artifact`   |           295 | `feat(desktop): collect bounded patch artifacts`        |
| R2.5a | `codex/r2-patch-artifact`     | `codex/r2-run-receipt`      |           747 | `feat(desktop): add verifiable bounded run receipts`    |

建议 PR body 统一包含：

- 当前审查单元的两个到三个行为变化。
- 明确的安全边界，例如不保存 Token、绝对路径、Patch 正文或原始 Runtime payload。
- 只列实际运行过的定向检查，并链接
  [R1/R2 验证记录](2026-08-30-runtime-host-r1-r2-verification.md)中的累计 Gate 证据。
- `Stacked on <base>`，说明 base PR 合并后再改回 `main`，不要把整个后续 stack 误审为本 PR。

## 仅本地、需明确上传许可

这些 PR head 尚无 `origin/*` 对应 ref。相邻的小提交已按实际累计 diff 合并成 10 个审查单元，
每个仍低于 800 changed lines。必须按下列顺序上传；不能把整个累计 head 强推成一个 PR，也
不能在未授权时重试。中间本地 branch 继续作为可回滚 checkpoint 保留，不必全部上传。

| 顺序    | Base                              | PR Head                           | Changed lines | 建议标题                                                |
| ------- | --------------------------------- | --------------------------------- | ------------: | ------------------------------------------------------- |
| R2.5b   | `codex/r2-run-receipt`            | `codex/r2-run-orchestration`      |           614 | `feat(desktop): persist and claim task runs atomically` |
| R2.5c   | `codex/r2-run-orchestration`      | `codex/r2-local-run-execution`    |           789 | `feat(desktop): orchestrate isolated Codex task runs`   |
| R2.5d   | `codex/r2-local-run-execution`    | `codex/r2-task-abandon`           |           582 | `feat(desktop): connect and reconcile local task runs`  |
| R2.5e   | `codex/r2-task-abandon`           | `codex/r2-task-supervisor`        |           754 | `feat(desktop): supervise local task runs`              |
| R2.5f   | `codex/r2-task-supervisor`        | `codex/r2-task-commands`          |           297 | `feat(desktop): expose local task commands`             |
| R2.5g   | `codex/r2-task-commands`          | `codex/r2-task-frontend-bridge`   |           598 | `feat(desktop): bridge local task state to React`       |
| R2.5h   | `codex/r2-task-frontend-bridge`   | `codex/r2-task-run-copy`          |           661 | `feat(desktop): add bounded task composer and run copy` |
| R2.5i   | `codex/r2-task-run-copy`          | `codex/r2-task-run-ui`            |           757 | `feat(desktop): render bounded task run receipts`       |
| R2.5j   | `codex/r2-task-run-ui`            | `codex/r2-task-workspace-cleanup` |           698 | `feat(desktop): make local tasks the project workflow`  |
| R2 Gate | `codex/r2-task-workspace-cleanup` | `codex/r2-gate-docs`              |           223 | `docs: record R1 and R2 verification status`            |

## 创建与合并纪律

1. 创建 PR 是对 GitHub 的外部写操作，最终提交前按工具安全规则一次性确认明确的 PR 集合。
2. 每个 PR 只比较表中的相邻 base/head，并确认 changed lines 低于 800。
3. 先审查和合并前一项，再把下一项 base 改为 `main`；不同时合并多个未重定基线的 PR。
4. PR #37/#38 保持 Draft 与暂停状态；不关闭、不合并，也不把其代码带入当前 stack。
5. 不恢复 Actions/CI 邮件；需要的验证证据来自本地 Gate 记录。
