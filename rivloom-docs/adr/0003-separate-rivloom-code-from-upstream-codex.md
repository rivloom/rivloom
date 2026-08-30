# ADR-0003：分离 Rivloom 产品代码并以合并方式同步上游

## 状态

Accepted

## 背景

Rivloom 仓库沿用 `openai/codex` 的上游源码和历史，需要长期吸收上游安全修复和 App
Server 新能力，同时维护自己的桌面界面、品牌和协作控制面。将产品功能散落在
`codex-rs` 中会显著增加同步冲突，也会让外部 Runtime 边界退化成源码耦合。

仓库根目录的 `AGENTS.md` 还要求避免在现有 `docs/` 中加入普通产品文档，因此 Rivloom
文档也需要清晰分区。

## 决策

- 桌面产品代码放在 `apps/desktop`。
- Rivloom 文档放在 `rivloom-docs`。
- 新的 Rivloom Rust 能力优先放在独立 crate，而不是 `codex-core`。
- 第一版优先使用现有 App Server v2 协议，不修改 Codex 核心。
- `apps/desktop` 不把 `codex-rs` 当作库内核；Codex 通过独立 App Server 进程接入。
- 第一条协作闭环完成前不进行大规模仓库拆分。之后可以单独评审把 Rivloom 产品代码与
  完整上游源码进一步分离，但不能让仓库迁移阻塞产品验证。
- 产品开发开始后，`main` 代表 Rivloom 产品状态。
- 上游更新在独立分支中合并 `upstream/main`，经过构建和测试后再通过 PR 进入 `main`。
- 不为维持线性历史而强制重写公开主分支。

## 结果

### 正面

- 大部分 Rivloom 功能与上游文件不重叠，减少同步冲突。
- 产品代码、上游代码和 Rivloom 文档边界清晰。
- 上游更新可以单独审查、测试和回滚。
- 社区能够看到每次同步带来的变化。

### 负面

- 上游前进且 Rivloom 已有提交后，更新通常会产生合并提交。
- 对同一上游文件的必要修改仍需人工解决冲突。
- 需要维护上游更新检查和测试流程。

### 中性

- 当前没有 Rivloom 功能提交时仍可使用快进同步。

## 考虑过的替代方案

**持续让 `main` 完全镜像上游，产品只放长期分支**

- 未采用：GitHub 默认分支不能直接代表 Rivloom 可构建产品，贡献和发布流程不直观。

**将 Codex 复制进新的独立仓库且不保留上游关系**

- 未采用：长期吸收上游修复困难，变更来源与许可证追踪也更复杂。

**通过 rebase 和强制推送保持线性历史**

- 未采用：会反复改写公开产品历史，增加协作者和发行追踪成本。

## 参考资料

- [Rivloom Desktop 架构设计](../plans/2026-08-24-rivloom-desktop-architecture-design.md)
- [Runtime Host 与协作闭环设计](../plans/2026-08-30-runtime-host-collaboration-design.md)
- [ADR-0005：采用外部 Agent Runtime](0005-use-external-agent-runtimes.md)
- Repository root `AGENTS.md`
