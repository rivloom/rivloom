# Rivloom Collaboration Protocol v1

状态：R3.1–R3.3 已合并复验，2026-08-31；R3.4 后端已本机验证、待合并；Gate R3 未通过。
本协议属于 Rivloom，不复用 App Server 原始消息，也不修改本地 R2 存储格式。

## 范围与入口

R3.1 只定义数据消息；不监听端口、不连接 Brain、不调用 Runtime、不处理邀请或授权。
实现位于 `apps/desktop/src-tauri/src/collaboration/protocol.rs`，仅在模块内使用。
网络接收方必须经 `Message::decode`，发送方经受校验的 `Message` 与 `encode`；
原始 `Envelope` 是未验证的构造材料，不能直接作为网络入口或发送对象。

- 每帧是一个 UTF-8 JSON 对象，含空白在内最多 **32 KiB**；接收在 JSON 解析前检查长度。
  R3.4 将 Message 嵌入最多 64 KiB 的 TLS 控制帧，分配前校验 4 字节长度，不使用压缩。
- 发送时校验字段和最终编码字节数。任何错误均拒绝整条消息，不截断、不降级、不部分执行。
- 所有对象/带标签分支拒绝未知字段；重复字段、未知枚举、非整数计数和未知版本被拒绝。
  不接受任意 JSON 扩展包、原始日志、环境变量、Runtime 凭证、路径或 App Server payload 字段。
- 对外错误只有 `InvalidMessage` 与 `MessageTooLarge`，不得记录原始解析错误或消息正文。
- 可空字段允许省略或 `null`，输出统一带 `null`；其余字段必填。数组保留顺序。

R3.4 控制帧与此 Message 封套独立：请求为 `{version:1,id,operation}`，
响应为 `{version:1,id,result}`；操作和结果使用 `type/data` 标签。
`id` 仅用于关联响应和拒绝同连接重复请求，不替代业务 `idempotencyKey`。
TLS 1.3 校验证书链/名称/时间及预先可信 leaf pin 后才发送认证 secret；无明文降级。
认证、邀请、owner 管理、pulse 和成员可见分页对账的边界见
[R3.4 验证记录](2026-08-31-runtime-host-r3-4-verification.md)。Brain 当前只接受自有 Node 声明及 draft Task，
Assignment/RunReceipt 的网络准入仍留给 R4；schema 或 pending 队列支持不等于执行授权。

## 消息封套

所有消息包含下列字段；`payload` 固定为 `{"type": "...", "data": {...}}`。

| 字段 | 契约 |
| --- | --- |
| `protocolVersion` | 整数，严格等于 1；未知版本直接拒绝，不能忽略或降级 |
| `messageId` | 本条消息 ID；原样重传保留 ID，新消息使用新 ID |
| `idempotencyKey` | 同一业务操作重试保持不变；不因为重连而生成新的执行授权 |
| `brainId` / `senderNodeId` | 所属 Brain 与本次发送 Node 的不透明 ID，不是认证证明 |
| `sentAt` | Unix 秒整数，范围 0–253402300799，不接受毫秒或浮点数 |
| `revision` | 0–9007199254740991；Node 发消息时为最后已应用的 Brain 修订号，Brain 发消息时为其权威修订号 |
| `payload` | 支持 `identity/node/task/assignment/runReceipt/artifact`；无批量/嵌套消息 |

所有 ID（含幂等键、项目引用）为 1–128 个 ASCII 字母、数字、下划线或连字符。
它们不能是 Windows/UNC/POSIX 路径。所有文字上限按 UTF-8 字节计算，非空文字不得全为空白。

## 基础数据 schema

| `type` | `data` 字段与限制 |
| --- | --- |
| `identity` | `identityId, memberId, deviceId, displayName, role`；名称 ≤80 字节，角色仅 `owner/member` |
| `node` | `nodeId, memberId, deviceId, runtimeId, runtimeVersion, capabilities`；Runtime 仅 `codex`，版本 ≤128 字节；能力最多 3 个且不可重复，仅 `taskRun/interrupt/patch` |
| `task` | `taskId, createdByMemberId, goal, constraints, expectedArtifact, status`；目标 ≤4096 字节；约束 ≤32 项，每项 ≤1024 字节、合计 ≤8192 字节；期望产物仅 `patch` |
| `assignment` | `assignmentId, taskId, offeredByMemberId, targetNodeId, executionPolicy, decision`；策略仅 `managedWorktreeOffline` |

Task 状态沿用 R2 的语义：`draft/offered/accepted/running/awaitingReview/approved/rejected/
cancelled/failed/outcomeUnknown`。这里只校验 schema，不实现状态转换。
Node 能力是声明，不证明已认证、在线或 Runtime 已验收；presence 和心跳过期由 R3.3 管理。

Assignment 的 `decision` 使用 `state` 标签，不能混用不同分支字段：

- `offered`：无其他字段。
- `accepted`：`acceptedByMemberId, projectRef, runId, runKey, acceptedAt`。
- `rejected/cancelled`：`decidedByMemberId, decidedAt`。

`projectRef` 仅由执行 Node 在本机映射到已登记项目；`runKey` 绑定唯一一次执行。
决定时间使用同一 Unix 秒范围。接收 Assignment 本身不会调用 Runtime。

## Artifact 与共享 RunReceipt

`artifact.data` 仅含以下元数据，字段顺序也是回执哈希中的顺序：
`artifactId, taskId, runId, baselineCommit, state, limitBytes, byteCount, sha256`。
前三者遵守 ID 限制；基线为 40 或 64 位小写十六进制 Git commit hash，不接受分支名或路径。
`limitBytes` 固定 524288（512 KiB），是未来 Patch 正文上限，不是元数据消息上限。

| `state` | `byteCount` / `sha256` |
| --- | --- |
| `empty` | 0 / 空字节串 SHA-256（`e3b0c442…b855`） |
| `complete` / `unsupportedEncoding` | 1–524288 / 64 位小写十六进制 SHA-256 |
| `tooLarge` | 两者均为 `null`；不可凭缺失数据猜测正文 |

不携带正文、下载 URL 或本机存储路径。元数据不能证明内容可用或哈希正确；
R5 读取真实正文后还须验证大小、hash、基线和可审查状态，不能自动接受超限/编码不支持产物。

`runReceipt.data` 是 `{content, contentSha256}`。这是面向共享的独立 DTO，
不是本地 R2 `RunReceipt` 的原样转发；本 PR 不改变 R2 存储或哈希，也不连接自动分享。

| `content` 字段 | 限制 |
| --- | --- |
| `taskId, runId, nodeId, runtimeId, runtimeVersion` | ID 同上；Runtime 仅 `codex`，版本 ≤128 字节 |
| `startedAt, finishedAt` | Unix 秒且结束不早于开始 |
| `outcome, summary, failure` | 结果为 `success/failed/cancelled/outcomeUnknown`；摘要可空，非空时 ≤4096 字节 |
| `tests` | `{state: notReported}` 或 `{state: reported, executions: [...]}`；执行项 `{name, exitCode}`，最多32项，名称每项≤256字节、合计≤4096字节，退出码为 i32 |
| `artifact` | 上述元数据；Task/Run ID 必须与回执相同，最多一个 Patch |

`failure` 仅接受 `executionFailed/connectionLost/policyDenied/invalidArtifact` 或 `null`，
不得包含原始错误文本。成功/取消必须为 `null`，失败/未知必须非空。
`notReported` 不等于测试通过；`success` 和空 Patch 也不能代替真实任务目标验收。

`contentSha256` 是共享 content 的确定性 JSON 字节的 SHA-256，小写十六进制：

1. 顶层 content 字段严格按上表顺序；tests 中先 `state` 再 `executions`，
   执行项先 `name` 再 `exitCode`，artifact 按上文顺序；数组顺序不变。
2. 使用紧凑 UTF-8 JSON，无空白、BOM 或换行；可空字段显式输出 `null`，整数十进制，
   非 ASCII 字符不转义、不做 Unicode 规范化；字符串按 serde_json 的 JSON 转义规则编码。
3. hash 不包括自身和消息封套，重发/重连即使改变封套，内容 hash 仍相同。
   六种 golden payload 固定往返结构，并用固定 hash 锁定序列化顺序。

接收方重算 hash 并验证内部关联。hash 是完整性检查，**不是身份认证、签名或执行证据**。
R4 后续从本地回执生成经确认可分享的内容，再计算并持久化共享 hash；不得拿本地旧 hash
给脱敏后的新 DTO 背书，也不能在重连时猜测或重新执行 Run。

## 授权、隐私与重放边界

Brain 后续必须根据已认证连接核对发送 Node、成员与 Brain，验证记录所有者、Task/Assignment/
Run 关联、目标 Node 的本地接受记录及修订号。声称 `owner`、提供 `acceptedByMemberId` 或提交
较大 `revision` 都不能成为自授角色、覆盖状态或执行任务的依据。

R3.3/R3.4 必须按 Brain、已认证发送者及幂等键限定重放域；相同键且内容相同返回已有结果，
相同键但内容不同应报冲突。同一 Run 重发不再次启动 Codex；乱序消息不回退状态，
断线无法证明结果时保留 `outcomeUnknown` 并先对账。本 PR 不声称已实现这些状态服务。

`managedWorktreeOffline` 仅声明现有单受管 worktree、禁网边界，不选择 Windows 沙箱实现，
也不改变当前 `on-request + auto_review`。任务接受不是无限权限授权，远端不得增加路径、
环境变量、网络开关或审批策略。未来扩权须独立设计/Gate，明确授权者、执行 Node、Task/Run、
资源、有效期、撤销及审计；当前没有万能权限字段。

自由文本可能由用户主动填入敏感信息；有界 schema **不等于秘密检测或 DLP**。
发送方必须只构造允许共享的字段，错误用封闭枚举，委派/回执分享前展示实际发送内容。
不得自动复制本地路径、账号或 Runtime 日志。最终模型 prompt 仍须经过 R2 的 4 KiB/1000-token
门禁；本协议上限不允许绕过它。私网加密、设备认证和 LAN 的 TLS/pin Gate 留在 R3.2–R3.4。

## 验证与后续

- R3.1a：`just test-rust` 219 项及 4 项命令测试通过；`just check` 95 项前端测试及
  TypeScript/Vite build 通过；普通/feature Clippy（`-D warnings`）及桌面 Rustfmt 通过。
- 仓库级 `just fmt` 已尝试，仍被既有失效 Python 启动器阻塞；不记为通过，
  不修改全局 Python 配置或 `codex-rs`。
- R3.1a 覆盖四种 golden 消息往返、未知版本/字段、重复字段、UTF-8 字节与集合边界、
  路径型 ID、越权字段、空分支额外字段和固定错误。
- R3.1b 已补齐共享回执/Artifact；所有字符串枚举只接受字符串，拒绝 Serde 对象别名。
  详细证据及 PR 关系见 [R3.1 验证记录](2026-08-31-runtime-host-r3-1-verification.md)。
- R3.2 已实现邀请、成员与 Node 凭证；R3.3 实现状态权威和存储，
  Gate R3 的两机认证/连接验收尚未通过。
- R1/R2 已合入 `8140f7c46b`；`R2-FU1` 的 elevated 多 Home 共存与真实执行/取消验收
  仍延期，必须在 Gate R4 和 Windows 可用性发布前完成，不以协议测试冒充。
- 不启用第二 Runtime、Marketplace、Skill Directory 或 CI，不处理旧 Draft PR #37/#38。
