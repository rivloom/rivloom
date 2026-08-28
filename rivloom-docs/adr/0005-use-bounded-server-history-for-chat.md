# ADR-0005：聊天只使用服务端有界历史和 Rust 权威生命周期

## 状态

Accepted

## 背景

Rivloom A3 需要恢复已有 thread、分页显示消息并订阅实时 turn/item 通知。桌面 sidecar
传输对单条 JSONL 设置了 4 MiB 硬上限；超限会断开连接并终止 sidecar。

当前稳定 `thread/resume` 默认返回完整 `thread.turns`。协议 spike 证明，一条 4 MiB 用户
消息即可让该响应超过桌面上限。实验 `thread/resume.excludeTurns` 和
`thread/turns/list` 能按条目分页，但都要求连接启用 `experimentalApi`，且即使
`limit: 1` 也没有响应字节预算。实验 `thread/items/list` 对当前本地 legacy rollout
仍不支持。

同时，账号通知已经使用 App Server 连接的唯一 `NotificationObserver`。如果 A3 直接
替换 observer、把原始通知发给 React，或只按 thread ID 过滤，断线重连、项目切换和迟到
delta 都可能污染当前会话。

## 决策

在桌面聊天实现之前，先提交一个独立、最小的 App Server v2 协议阶段：

- 稳定现有 metadata-only resume 能力，默认完整历史行为保持兼容；
- 稳定 summary/notLoaded 的 `thread/turns/list` 路径；
- 为 history result 增加客户端字节预算和服务端安全上限；
- summary 只包含每个 turn 的首条用户消息和最终助手消息，对所有内容做 UTF-8 安全
  截断，并通过 `truncatedTurnIds` 显式报告；
- 页在预算前结束并返回可前进游标，任何有效请求的完整结果都不得击穿桌面 4 MiB；
- A3 不稳定或使用 `thread/items/list`、`initialTurnsPage` 和其他实验能力。

Rivloom 不扩大 JSONL 上限，不读取 App Server 内部 rollout，也不把 A3 缩减为只支持新
thread。该协议变更是 ADR-0004“默认不修改 `codex-rs`”的有证据例外，并作为独立 PR
落地，避免与桌面实现混合评审。

桌面后端安装一个固定 `ConnectionRouter`，作为唯一连接 observer、通知 observer 和
服务端请求 handler。它把连接生命周期与账号通知交给 `AccountService`，把连接生命周期、
聊天通知和反向请求交给 `ChatService`。额度通知先由 `AccountService` 归一化，再把固定
快照交给 `ChatService`；reader 通知回调不得同步重入连接。`ChatService` 以
`ConnectionIdentity + lifecycleRevision + projectId + threadId + turnId + itemId` 验证
事件，在 Rust 中完成历史合并、delta 批处理、turn/item reducer、重连对账和服务端请求
拒绝。React 只接收有界、脱敏、固定 union DTO。

实时通知不新增上游协议。桌面在 4 MiB decoder 之后借用读取原始 `params`，只复制通过
字段和总量上限的归一化内容；完成事件也不能把被丢弃的工具参数、输出或超长文本重新写回
状态。单条实时 JSONL 若超过 4 MiB，按断线处理并通过有界历史对账，不扩大 decoder。

A4 前 thread start/resume 使用稳定 `sandbox: "read-only"`，turn start 使用稳定
`sandboxPolicy: { type: "readOnly", networkAccess: false }`，三者都固定
`approvalPolicy: "never"` 并验证响应中的有效策略；不启用实验 `permissions`。固定基线
中的全部反向请求都由 Rust 明确拒绝、取消或返回受控的不支持错误，React 不获得批准入口。

## 结果

### 正面

- 已有和新 thread 都能在明确字节契约下恢复，不会用内存或传输上限掩盖协议问题。
- 旧连接和迟到事件在 Rust 边界被隔离，账号与聊天安全复用同一连接。
- 上游协议改动很小、兼容旧客户端，并有公共 JSON-RPC 测试可独立评审。
- React 状态不依赖任意 JSON，长会话、日志和安全上限可以确定性测试。

### 负面

- A3 增加一个必须先合并的 App Server 协议 PR。
- 历史 summary 是有损视图；A3 不显示旧推理全文和旧工具完整输出。
- App Server 和桌面必须共同维护字节预算、截断和游标契约。

### 中性

- 实时运行仍使用稳定 turn/item 通知，只有历史读取需要新的有界契约。
- A4 可以在相同 router 和 reducer 上增加审批与 Diff，无需改变 A3 的历史所有权。
- 未来稳定 items 分页后可按独立 ADR 扩展历史工具详情。

## 考虑过的替代方案

**只支持新或空 thread**

- 未采用：规避了有界恢复问题，但不满足 A3 打开已有 thread 的目标。

**启用现有实验 API**

- 未采用：连接级扩大实验面，且没有单条或整页字节保证。

**提高桌面 JSONL 上限或改为直接读取 rollout**

- 未采用：只把无界输入推迟到更大内存，或绕过公开协议和 sidecar 数据所有权。

**让 React 直接消费原始通知**

- 未采用：扩大攻击和兼容面，无法集中执行竞态隔离、截断、脱敏和 A4 前拒绝策略。

## 参考资料

- [A3 协议 spike](../plans/2026-08-28-desktop-chat-streaming-protocol-spike.md)
- [A3 聊天与流式运行设计](../plans/2026-08-28-desktop-chat-streaming-design.md)
- [ADR-0004：以稳定 cwd 协议表示本地项目](./0004-use-stable-cwd-for-local-projects.md)
- `codex-rs/app-server/README.md`
- `codex-rs/app-server-protocol/src/protocol/v2/thread.rs`
- `apps/desktop/src-tauri/src/app_server/connection.rs`
