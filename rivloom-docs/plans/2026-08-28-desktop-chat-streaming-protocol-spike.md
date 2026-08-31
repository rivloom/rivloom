# Rivloom Desktop A3 聊天与流式运行协议 Spike

- **日期：** 2026-08-28
- **基准：** `22db844b5c459dcb1415c45577013a3b171ff497`
- **结论：** A3 不能直接基于当前稳定协议实现有界历史恢复。必须先完成一个独立、最小的 App Server 有界历史协议阶段；桌面端不得通过扩大 4 MiB 上限、读取 rollout 文件或静默启用实验 API 绕过该门槛。

## 1. 目的和限制

本 spike 回答 A3 在正式设计前必须确认的协议问题：历史响应是否有界、恢复后是否持续订阅、释放是否终止 turn、实验能力如何门控、服务端请求被拒绝后的状态，以及 `clientUserMessageId` 是否具有幂等语义。

所有运行验证均使用临时隔离的 `CODEX_HOME`、合成 rollout、本机回环 mock Responses 服务或桌面 fake transport。没有登录真实账号、没有发起真实模型请求、没有读取用户 rollout。临时测试在取证后已移除，`codex-rs` 和桌面产品代码均恢复为基准内容。

## 2. 已确认的稳定能力

| 能力 | 证据 | A3 含义 |
| --- | --- | --- |
| `thread/resume` | v2 稳定方法；默认恢复并返回 `thread.turns` | 能恢复和订阅，但默认历史响应无界 |
| `thread/read { includeTurns: false }` | v2 稳定 metadata 读取；公共测试确认 `turns` 为空 | 可在 resume 前校验 thread ID/cwd，必须显式禁止 turns |
| `thread/unsubscribe` | 稳定方法；返回 `notLoaded`、`notSubscribed` 或 `unsubscribed` | 可在切换项目、thread 和退出时释放订阅 |
| `turn/start` | 稳定方法；响应初始 turn，并流式发送 turn/item 通知 | 可用于 A3 文本发送 |
| `turn/interrupt` | 稳定方法；成功响应为空对象 | 只表示中止请求被接受；仍须等待 `turn/completed` |
| turn/item 生命周期 | `turn/started`、`item/started`、item delta、`item/completed`、`turn/completed` | `item/completed` 和 `turn/completed` 分别是权威终态 |
| 账号和额度通知 | `account/updated`、`account/rateLimits/updated` | 可由同一后端通知路由器分发，不应建立第二条 sidecar 连接 |
| 额度快照 | 稳定 `account/rateLimits/read` + 稀疏 `account/rateLimits/updated` | Rust 归一化固定快照；reader 通知回调不得重入连接等待 read 响应 |

安全字段并非三个方法共用同一 wire 类型：`thread/start` 和 `thread/resume` 接受稳定
`sandbox: "read-only"`，Core 将其投影为磁盘只读、网络关闭；`turn/start` 接受稳定
`sandboxPolicy: { type: "readOnly", networkAccess: false }`。三者都接受稳定
`approvalPolicy: "never"`。A3 必须对不同方法构造精确 payload，不能互换 `sandbox` 与
`sandboxPolicy`，也不应借助实验 `permissions` 达到同一目的。

## 3. 实验能力和当前缺口

下列能力都要求初始化时声明 `capabilities.experimentalApi: true`：

- `thread/resume.excludeTurns`
- `thread/resume.initialTurnsPage`
- `thread/turns/list`
- `thread/items/list`

关闭实验能力时，临时公共 JSON-RPC 测试确认：

- 带 `excludeTurns: true` 的 `thread/resume` 返回 `-32600`，消息为 `thread/resume.excludeTurns requires experimentalApi capability`；
- `thread/turns/list` 返回 `-32600`，消息为 `thread/turns/list requires experimentalApi capability`。

启用实验能力时，`excludeTurns`、初始 turns 页和前后游标能按当前实现工作，但 `thread/items/list` 对当前本地 legacy rollout 仍返回“不支持”。因此 A3 既不能把实验 API 当作稳定依赖，也不能依赖 items 分页补齐历史工具详情。

## 4. 4 MiB 关键实证

桌面 JSONL decoder 的硬上限为 `4 * 1024 * 1024` 字节；超限会产生 `LineTooLarge`，连接监督器随后断开请求并终止 sidecar。

临时 App Server 集成测试创建了一条内容恰为 4 MiB 的用户消息，然后测量公共 JSON-RPC 响应：

1. 稳定 `thread/resume` 的完整历史响应超过 4 MiB；
2. 实验 `thread/turns/list` 使用 `limit: 1`、`itemsView: "full"` 时仍超过 4 MiB。

测试通过意味着两个“大于”断言成立。结论是：条目数量分页不等于字节有界；单条消息、工具输出或错误就能击穿传输上限。当前 App Server 没有客户端响应字节预算、稳定 metadata-only resume 或历史截断标记。

## 5. 恢复、订阅和释放实证

以下现有 App Server 公共 JSON-RPC 测试在本基准通过：

- `thread_resume_can_skip_turns_for_metadata_only_resume`
- `thread_resume_initial_turns_page_matches_requested_turns_list_page`
- `thread_turns_list_can_page_backward_and_forward`
- `thread_resume_keeps_in_flight_turn_streaming`
- `thread_unsubscribe_keeps_thread_loaded_until_idle_timeout`
- `thread_unsubscribe_during_turn_keeps_turn_running`
- `thread_items_list_returns_unsupported`

由此确认：resume 会建立当前连接的订阅；运行中的 turn 可被另一连接重新加入并继续收到流；unsubscribe 只释放当前连接的订阅，不会中止 turn，也不会立即卸载 thread。桌面必须把“释放订阅”和“中止 turn”建模为两个独立动作。

## 6. A4 前的服务端请求

桌面当前对未知服务端请求返回 `-32601 method not supported`。临时测试确认 `request_user_input` 收到该错误后，会先发 `serverRequest/resolved`，随后 turn 能正常完成。

命令审批和文件变更使用仓库已有拒绝测试验证；为避免沙箱环境宏提前跳过，spike 临时移除了本机 mock 用例的 `skip_if_no_network!`，实际用例约 0.9 秒完成后恢复源文件：

- 命令明确拒绝得到 `commandExecution.status: "declined"`；无效响应得到 `"failed"`；
- 文件变更明确拒绝和无效响应都得到 `fileChange.status: "declined"`；
- 被拒绝的文件不存在，turn 最终完成。

A3 应继续使用 `approvalPolicy: "never"`、只读 sandbox 和关闭网络作为第一层防线；若仍收到反向请求，Rust 后端按已知方法返回明确拒绝/取消，未知方法才返回 `-32601`。React 不接收原始请求，也不提供批准入口。

## 7. 发送去重实证

临时测试用相同 `clientUserMessageId` 连续调用两次 `turn/start`，两次都完成且产生不同 turn ID。因此该字段只用于把服务端 `userMessage.clientId` 与本地消息关联，不是幂等键。

桌面在请求结果未知时不得自动重发。草稿发送必须在 Rust 中维护一次性发送记录；重连后先对账，再由用户明确选择是否重新发送。

## 8. 协议门槛和建议

A3 的完整目标包括已有 thread 的有界恢复，不能缩减为只支持新 thread。推荐先提交独立 App Server 协议 PR，满足以下可验证契约：

1. metadata-only resume 成为稳定能力，且仍建立订阅；
2. 稳定历史分页以 summary 视图返回每个 turn 的首条用户消息和最终助手消息；
3. 服务端对 summary 字段执行 UTF-8 安全截断，并对整页执行序列化字节预算；
4. 每页无论正常、超长单条或错误，完整 JSONL 均严格小于桌面 4 MiB；
5. 响应显式指出截断，不允许游标停滞；
6. legacy 和 paginated rollout、冷恢复和运行中恢复都使用同一契约；
7. 先不稳定 `thread/items/list`，A3 历史不恢复旧工具输出和推理全文。

如果协议 PR 无法在不破坏兼容性的前提下满足这些条件，A3 必须停在设计门槛，不能通过放宽桌面上限继续。

## 9. 未由本 spike 直接证明的内容

下列内容应由 A3 桌面 fake-sidecar 集成测试覆盖，而不是据当前 App Server 用例推断：

- resume 超时后迟到响应是否留下未知订阅；
- sidecar 断线期间 delta 的丢失和重放边界；
- 旧连接、旧项目、旧 thread 和旧 turn 通知是否被桌面隔离；
- 应用退出时 unsubscribe 的时间预算和失败行为；
- Windows、macOS、Linux 上草稿持久化、原子替换和日志脱敏。
