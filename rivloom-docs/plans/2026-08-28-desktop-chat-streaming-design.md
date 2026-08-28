# Rivloom Desktop A3 聊天与流式运行设计

- **状态：** 已确认
- **日期：** 2026-08-28
- **基准：** `origin/main` 的 `22db844b5c459dcb1415c45577013a3b171ff497`
- **前置证据：** [A3 协议 spike](./2026-08-28-desktop-chat-streaming-protocol-spike.md)

## 1. 目标与成功标准

A3 在 A2 的本地项目和 thread 列表上建立安全的文本聊天闭环。用户可以打开已有 thread、看到有界历史、发送一条文本消息、实时查看助手文本、推理摘要和工具状态、中止当前 turn，并在切换项目或连接重建后得到可解释的状态。

成功必须同时满足：

- 所有 App Server JSONL 都受 4 MiB 传输边界保护，历史和 React 内存均有硬上限；
- 旧连接、旧项目、旧 thread、旧 turn 和旧 item 的事件不能污染当前界面；
- 请求超时或断线时不自动重发，不把未知结果伪装成失败；
- A4 前不能修改文件、访问网络或由 React 批准服务端请求；
- Windows、macOS、Linux 使用同一状态机和路径语义；
- 产品实现不直接读取 rollout，不把 App Server 原始 JSON 暴露给 React。

## 2. 范围和非目标

### 2.1 A3 包含

- 已有 thread 的 metadata-only resume、订阅、有界消息摘要分页和释放；
- 新建 thread 后的文本 `turn/start`；
- 用户消息、助手文本 delta、推理摘要、命令和其他工具的只读状态；
- `turn/interrupt` 和 `turn/completed` 权威终态；
- 断线、重连、项目切换、thread 切换和应用退出生命周期；
- 每个 thread 的本地草稿、发送关联和额度/连接错误展示；
- 长列表虚拟化、增量批处理、数据和日志脱敏；
- 作为 A3.0 的最小 App Server 有界历史协议前置阶段。

### 2.2 A3 不包含

- A4 的命令/文件审批、Diff 展示、接受补丁或写入文件；
- 网络访问、MCP/插件、动态工具、应用连接器和远程副作用；
- 图片、音频、文件附件、实时语音、`turn/steer` 和实验 queue；
- 多个 thread 同时订阅或同时运行；切走的 turn 可在服务端继续，但 A3 不后台展示；
- 历史推理全文、历史工具完整输出或 rollout 级审计；
- 直接修改上游 Core 推理逻辑、模型请求格式或启用其他实验 API。

## 3. 方案比较

### 方案 A：只用当前稳定 API，已有 thread 继续不可聊天

优点是完全不改 App Server。缺点是稳定 resume 默认返回完整 turns，超长历史会断开 sidecar；只支持新 thread 会违反 A3 的恢复目标。因此不采用。

### 方案 B：桌面静默启用实验分页

`excludeTurns` 和 `thread/turns/list` 已能工作，但都受实验能力门控；单条消息在 `limit: 1` 时仍可超过 4 MiB，`thread/items/list` 对当前本地历史也未支持。它不能提供稳定或字节有界契约。因此不采用。

### 方案 C：先落最小有界历史协议，再实现桌面（推荐）

只稳定 A3 必需的 metadata-only resume 和 summary turns 分页；新增结果字节预算和显式截断，不稳定 items 分页。协议 PR 与桌面 PR 分离。代价是 A3 多一个前置阶段，但这是唯一同时满足完整需求、兼容性和 4 MiB 边界的路线。

## 4. 高层架构

```text
App Server sidecar
  │ JSON-RPC（单连接、Rust 内解析）
  ▼
ConnectionRouter ───────────────► AccountService
  │ connection identity             账号、额度
  ▼
ChatService
  ├─ lifecycle/session revision
  ├─ bounded history pager
  ├─ turn/item reducer
  ├─ delta coalescer
  ├─ draft/submission store
  └─ safe server-request rejector
  │ 固定 Tauri DTO / event
  ▼
chatBridge + useChatSession
  ▼
virtualized React transcript + composer
```

`ConnectionRouter` 是监督器唯一的连接、通知和服务端请求路由对象。它把连接建立/断开同时送给 `AccountService` 与 `ChatService`，按方法族分发通知，并把反向请求只送给 `ChatService` 的安全拒绝器。`account/rateLimits/updated` 先由 `AccountService` 归一化，再把固定额度快照交给 `ChatService`；通知回调不得在 reader 线程同步发起 JSON-RPC 请求。路由器固定安装一次；服务切换只改变各服务内部 revision，不替换 observer 或 handler，避免覆盖和安装竞态。

`ChatService` 是协议和安全边界。React 只得到归一化 DTO，例如 `historyReplaced`、`itemsChanged`、`turnStateChanged` 和 `usageChanged`，不接触 method 名、任意 JSON、服务端请求 ID、工具参数或完整错误。

## 5. A3.0 有界历史协议前置

独立 App Server PR 应只做下列兼容性扩展：

1. 将现有 `thread/resume.excludeTurns` 稳定化；默认值仍为 `false`，旧客户端行为不变；
2. 将 `thread/turns/list` 的 summary/notLoaded 路径稳定化；A3 不使用 `full`；
3. 请求新增可选 `maxBytes`。设置时，`result` 序列化后不得超过该值；服务端同时设置安全最大值；
4. summary 每个 turn 只保留首条用户消息和最终助手消息，并对所有字符串执行 UTF-8 安全的总量截断；
5. 响应新增 `truncatedTurnIds`，明确哪些 turn 的 summary 被截断；
6. 若加入下一个 turn 会超过页预算，则在它之前结束本页并返回能继续前进的 `nextCursor`；首个 turn 经固定字段上限后必须能装入最小允许预算；
7. legacy rollout、paginated rollout、冷 thread 和运行中 thread 使用相同约束并有公共 JSON-RPC 集成测试；
8. 保持 `thread/items/list` 和 `initialTurnsPage` 为实验能力，A3 不声明实验 API。

桌面请求 20 turns、summary、`maxBytes: 3 MiB`，为 JSON-RPC envelope、ID 和换行保留至少 1 MiB。App Server 测试必须对 0、1、20 turns，超长 UTF-8 文本、错误和工具历史逐一断言结果字节数与游标前进。

## 6. thread 完整生命周期

### 6.1 打开

1. 后端接收 `projectId` 和 `threadId`，不接受前端 cwd；
2. 增加 `lifecycleRevision`，立即使旧 revision 失效；
3. 核对项目仍在后端 registry，并用 `thread/read { includeTurns: false }` 验证 thread cwd；该请求只取 metadata，禁止使用完整 turns；
4. 对旧 thread 执行有时限的 best-effort unsubscribe；这不会中止旧 turn；
5. 在当前连接调用 `thread/resume { excludeTurns: true, cwd }`；核对返回 thread ID 和 cwd；
6. resume 建立订阅后，请求最新 20 turns 的有界 summary 页；历史装载期间缓存匹配通知；
7. 将降序页反转为时间顺序，应用缓存事件，再发布 `ready`。

任一步失败都只影响当前 revision。若 resume 超时，订阅结果未知：先使 revision 失效，再用同一连接 best-effort unsubscribe；禁止在原请求上重试。

### 6.2 更早历史

滚动到顶部时用当前 `nextCursor` 请求一页。每次只允许一个历史请求；游标必须变化，否则进入可恢复协议错误。新页按 turn ID 去重后前插。超过窗口上限时优先移除最远离当前视口的已完成旧页，保留重新装载游标。

### 6.3 切换和退出

项目或 thread 切换先失效 revision、停止向 React 发布、清空 delta 批次，再 best-effort unsubscribe。应用退出给释放动作固定短预算，随后由 supervisor 终止 sidecar；不得为了等待 unsubscribe 无限阻塞退出。

### 6.4 断线和重连

断线立即把活动 turn 标为 `outcomeUnknown`，不改成 failed，不恢复草稿，不自动重发。新连接到达后创建新 revision，metadata-only resume，再重新读取最近有界页并与本地窗口按 `(turnId,itemId)` 对账；缓存在此期间到达的新连接通知。对账结果可以回到 `inProgress` 或三个终态之一。

## 7. 竞态隔离键

每个异步响应和通知至少携带或绑定：

```text
ConnectionIdentity
  + lifecycleRevision
  + projectId
  + threadId
  + turnId（turn/item 事件）
  + itemId（delta/item 事件）
```

缺少当前 thread/turn/item 上下文、来自旧连接、revision 不匹配、项目不匹配或已完成 item 的迟到 delta 全部丢弃。丢弃只记录计数和方法类别，不记录 payload。不能仅凭 `threadId` 接受通知，因为项目切换、断线重连和迟到响应都可能复用它。

## 8. 状态机

### 8.1 会话状态

```text
closed → loading → ready
            │        │
            └→ error │
                     ├→ reconnecting → reconciling → ready/error
                     └→ releasing → closed
```

`loading` 与 `reconciling` 都缓存有硬上限的匹配事件；溢出时丢弃缓存并重新读取权威 summary 页，不把不完整流直接展示。

### 8.2 turn 状态

```text
idle → starting → inProgress → completed
          │           ├──────→ failed
          │           ├──────→ interrupting → interrupted/failed/completed
          └→ sendFailed

starting/inProgress/interrupting → outcomeUnknown → reconciling
```

`turn/start` 响应或匹配的 `turn/started` 都可把 starting 提升为 inProgress。`turn/interrupt` 的空响应只进入 interrupting；只有 `turn/completed` 决定 completed、failed 或 interrupted。请求在确认送达前失败可恢复草稿；超时或断线属于 outcomeUnknown，不能恢复为“未发送”。

### 8.3 item 状态

`item/started` 创建 item，匹配 delta 追加到 Rust 侧缓冲，`item/completed` 用完整 item 替换并封存。未见 started 的完成事件可直接补建；未见 item 的 delta 在小型待定表中短暂缓存，超时或超限后丢弃并触发对账。`turn/completed` 不能代替完整 item 列表，只可补充最终助手摘要。

## 9. React 消息模型和性能边界

Rust 将 App Server item 归一化为有限 union：用户消息、助手消息、推理摘要、命令状态、通用工具状态和安全阻止提示。工具参数、完整输出和任意 JSON 不进入 React。

初始硬上限：

| 对象 | 上限 |
| --- | --- |
| 文本输入 | 32 KiB UTF-8 |
| 草稿 | 20 个、合计 256 KiB |
| 历史页 | 20 turns、App Server result 3 MiB |
| React 当前窗口 | 200 turns 或 8 MiB 归一化数据 |
| Rust 当前会话窗口 | 同样为 200 turns 或 8 MiB 归一化数据 |
| 游标、ID | 分别 4 KiB、1 KiB |
| 单条显示文本 | 128 KiB |
| 命令/工具显示摘要 | 每项 8 KiB；参数、结果和聚合输出不保留 |
| 用户可见错误 | 8 KiB |
| loading 事件缓存 | 512 事件或 2 MiB |
| 未见 item 的待定 delta | 32 个 item 或 256 KiB，超过 5 秒或溢出即丢弃并触发对账 |
| 单次流式 Tauri 批次 | 128 个变更或 256 KiB，最多约 30 批/秒 |

Rust 对实时 `params` 使用借用读取，只复制通过类型、身份和字段上限校验后的 DTO；完整工具参数/结果、聚合输出和原始错误不得克隆进状态。达到单 item 上限后继续消费但不再追加内容，并设置归一化截断标记；`item/completed` 也必须经过同一投影，不能用完整 item 覆盖有界状态。Rust 最多约 30 Hz 合并 delta 后向 Tauri 发送有界批次；React reducer 一次应用一批。消息区使用虚拟列表和稳定 item key。虚拟化只解决 DOM 数量，不能替代 Rust 与 React 各自的 8 MiB 内存硬上限。

草稿按 thread ID 存入 Rivloom 隔离目录的版本化 JSON，使用跨平台同目录临时文件、flush 和原子替换；不复制 App Server 历史。发送时 Rust 生成 `clientUserMessageId`，同一发送记录只允许一个在途请求。该 ID只做关联，服务端不提供幂等。

## 10. A4 前的安全边界

每次 thread start/resume 和 turn start 都强制使用各自稳定 wire 形状：

- `thread/start` 与 `thread/resume` 发送 `approvalPolicy: "never"`、`sandbox: "read-only"`；它们不接受 `sandboxPolicy`，粗粒度 read-only 在 Core 中等价于磁盘只读且网络关闭；
- `turn/start` 发送 `approvalPolicy: "never"`、`sandboxPolicy: { type: "readOnly", networkAccess: false }` 和后端解析出的项目 cwd；它不使用 thread 请求的 `sandbox` 字段；
- thread start/resume 响应中的有效 `approvalPolicy` 和 `sandbox` 必须仍为 never/read-only/network-off，否则会话进入不安全配置错误且禁止发送；
- 不声明 `permissions`、dynamic tools、MCP UI、attestation、apps、插件、environments、collaboration mode 或实验能力。

Rivloom 使用 ADR-0002 的隔离 `CODEX_HOME`，A3 不写入 MCP 配置。已知反向请求在 Rust 后端按方法返回明确 decline/cancel；`request_user_input` 和未知方法可返回受控错误。React 永远没有“批准”命令。`fileChange` 只显示“本阶段已阻止”，不展示 diff；命令只显示已脱敏命令摘要和最终状态。任何可能有外部副作用且无法证明被 read-only/network-off 覆盖的工具类型都在 A3 失败关闭。

## 11. 错误、额度、持久化和日志

- `error` 通知中 `willRetry: true` 只显示暂时重试，不结束 turn；终态仍看 `turn/completed`；
- 额度来自账号服务对稳定 `account/rateLimits/read` 和 `account/rateLimits/updated` 的归一化快照；只保留窗口百分比、重置时间和明确的耗尽状态，不保留 bucket 名、credits 明细或任意 JSON；
- 打开会话时可从命令/后台线程刷新额度，通知 reader 回调只归一化已收到的 payload，绝不重入同一连接等待响应；所有额度响应绑定 connection identity 和账号 revision，迟到结果丢弃；
- 明确耗尽时禁止新发送，不影响历史浏览和中止；额度未知或刷新失败时显示警告，但仍允许用户主动发送并由 App Server 执行最终额度校验，绝不自动发送或自动重试；
- App Server 是 thread/turn 历史权威源，桌面只持久化草稿和小型 UI 偏好；
- 日志不记录提示词、delta、推理、工具参数/输出、服务端请求 payload、token、完整路径或原始错误；
- 诊断只记录连接代次、revision、事件类别、计数、截断标志和稳定错误码；
- 所有字符串先按 UTF-8 边界截断再进入状态、事件或日志。

## 12. 跨平台和失败模式

| 失败 | 行为 |
| --- | --- |
| 历史结果超过协议预算 | App Server 在发送前截断或缩页；若契约失效，桌面显示协议错误而不是提高上限 |
| 单条实时 JSONL 超过 4 MiB | 现有 decoder 关闭该连接；活动 turn 进入 outcomeUnknown，重连后用有界 summary 对账，不提高上限 |
| resume 超时后迟到 | revision 已失效；丢弃响应并 best-effort unsubscribe |
| 切换时旧 delta 到达 | 连接身份/revision/thread/turn/item 任一不匹配即丢弃 |
| unsubscribe 失败 | 当前 UI 仍关闭；依赖 identity 隔离，后台记录计数，重连时对账 |
| 活动 turn 断线 | outcomeUnknown；重连 resume + summary 对账 |
| 额度响应迟到、畸形或暂时不可用 | identity/revision 不匹配即丢弃；当前快照变为 unknown，显示警告且不自动发送 |
| 草稿写入中崩溃 | 保留旧正式文件；临时文件下次启动清理 |
| 路径来自远程执行 OS | 只当显示字符串，不用本机路径 API 解释 |
| 工具请求越权 | 后端拒绝，item 显示 declined/failed；不交给 React |

## 13. 测试和验收

协议阶段使用 App Server 公共 JSON-RPC 集成测试，覆盖稳定能力门控、序列化字节预算、UTF-8 截断、legacy/paginated 历史、游标和运行中恢复。

桌面 Rust 使用 fake connection/fake sidecar 覆盖：metadata-only read、打开/分页/释放、旧 identity、旧 revision、迟到响应、额度稀疏更新、缓存溢出、delta 合并、三种终态、interrupt、断线对账、服务端请求拒绝、草稿原子替换和日志脱敏。

React 使用 bridge、hook、reducer 和组件测试，覆盖窗口硬上限、重复/乱序事件、发送一次性、outcomeUnknown、虚拟列表、截断提示和额度错误。所有用户可见 UI 必须有快照；最终在 Windows 做 sidecar smoke 和视觉验收，并在 CI 证明 macOS/Linux 的平台无关测试。

## 14. 设计结论

A3 先做协议、再做桌面，不启用现有实验 API，不扩大 4 MiB，不读取 rollout。协议只稳定 metadata-only resume 和有字节预算的 summary turns 分页；桌面采用固定通知路由器、Rust 权威 reducer、严格隔离键和重连对账。A4 前所有运行保持只读、断网和不可审批。
