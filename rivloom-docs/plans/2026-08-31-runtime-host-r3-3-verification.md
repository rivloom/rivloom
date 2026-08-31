# Rivloom Runtime Host R3.3 验证记录

日期：2026-08-31。状态：实现中，Gate R3 未通过。

## 基线与拆分

R3.2 #69/#70 已顺序普通 merge，主干为 `0245711c76`。
独立主干 worktree 的 238 + 4 Rust、95 前端测试与 build、两组 Clippy、
桌面格式检查和 diff 均通过。没有管理员绕过、force-push 或恢复 CI。
R3.3 从该最新 origin/main 创建 `C:/project/opencohive/.worktrees/r3-3-brain-state`；
主目录旧 main 及原有 Cargo.toml 改动不动。

计划拆分为各自小于 800 行的依赖 PR：

1. R3.3a：[PR #71](https://github.com/rivloom/rivloom/pull/71)，403 行；
   凭证/邀请状态的有界、严格恢复，拒绝重复键、错误身份关联、TTL 和撤销不一致。
2. R3.3b：[PR #72](https://github.com/rivloom/rivloom/pull/72)，610 行；
   Brain 成员/Node/presence 和唯一修订号。
3. R3.3c：Task 状态、发送者限定的重放与乱序保护。
4. R3.3d：单写者、原子快照和失败恢复；超过 800 行时继续拆开审查单元。

## 实现边界

- 磁盘快照只保存允许的有界协作数据与 secret verifier，不保存 bearer secret 或 Runtime 数据。
- 读取损坏/未知版本不得降级为空 Brain；初始化与恢复是不同入口。
- 邀请消费、成员/凭证签发必须在同一快照提交后才向调用方返回 secret。
- 单 Brain 对可观察变更发出统一修订号；重试同键同内容返回原结果，同键不同内容冲突。
  不驱逐幂等历史后悄悄重执行；达到容量时拒绝新增操作。
- 心跳以 Brain 收到时间计算有效期；离线不推断 Task 成败，恢复时不把旧心跳当作活连接。
- 这里仍是本地核心。网络认证、加密、连接关闭/对账和两机 Gate R3 留在 R3.4；
  R4 才接真实委派、接受关联与 Run 结果，不从远端任意状态声明授予执行权限。

## 不可跳过的限制

R3.3a 验证：新增 4 项恢复行为测试，`just test-rust` 242 + 4、
`just check` 95 + build、两组 Clippy（`-D warnings`）通过。
验证包含撤销/时钟/过期的恢复、身份与 TTL 不一致、重复键、容量和邀请消费重放。
单独的注册表反序列化只供快照内部使用，完整文件的字节限长与完整性由后续存储层完成。
快照哈希用于检测意外损坏，不提供对本机账号恶意篡改/回滚的密码学防护。

R3.3b 已实现本地 owner 初始化、邀请/成员/Node 的统一权威、撤销和 30 秒 presence。
bootstrap 的 owner 只能在本机创建；首版没有远端自授 owner、owner 转移或 owner 撤销入口。
成员名称限制为协议 v1 的 80 UTF-8 字节；identity/device 仍是自报标签。
可观察事务统一递增 Brain 修订号；同一原子变更涉及的成员与 Node 使用同一修订号。
旧修订号和修订号耗尽拒绝写入；有界完整快照校验所有身份、成员/Node、撤销和时钟关联。
恢复入口不隐式建立活连接；持久服务打开时必须执行 Restart presence 清理并提交。
新增 5 项 Brain 测试，`just test-rust` 247 + 4、`just check` 95 + build、
两组 Clippy（`-D warnings`）、桌面格式与 diff 检查通过。原子提交仍由后续存储 PR 完成。

R3.3c 增加受认证发送者限定的消息准入，只接受匹配自身绑定的 Node 公告/心跳及新 draft Task。
Identity/owner、Assignment、RunReceipt、Artifact 和远端非 draft Task 不能借此入口写入权威状态。
Task 后续状态只能由受信本地协调逻辑推进；R4 必须先校验接受、Run 和回执关联，此函数本身不执行 Runtime。
幂等域为当前 Brain + 已认证 Node + key，比较规范序列化 payload 的 SHA-256；
messageId、sentAt、最后已见 revision 不参与业务 fingerprint，重试返回原修订号。
认证和撤销检查始终在缓存命中之前执行；重复心跳不更新 lastSeen，可信时钟仍可前进。
新操作要求当前 Brain revision；同键不同 payload、重复 Task ID、乱序和越权声明均拒绝。
最多 64 Tasks、256 条重放记录；达到容量拒绝新操作，不自动驱逐历史。尚无清理/压缩产品入口。
恢复保留 Task 状态和重放结果；presence 过期/重启不会推断 Task 成败，也不会重新调用 Runtime。
新增 7 项 Task/重放行为测试，`just test-rust` 254 + 4、`just check` 95 + build、
两组 Clippy（`-D warnings`）、桌面格式与 diff 检查通过。

`R2-FU1` Windows elevated 多 Home 共存和真实执行/取消、边界/cleanup 验收继续延期，
Gate R4/Windows 可用性发布前必须补齐。当前仍 `on-request + auto_review`。
不改 codex-rs、不恢复 CI、不处理 #37/#38，不提前引入第二 Runtime/Marketplace/Skill Directory，
不使用子 agent。仓库级格式化与 Bazel 锁更新的既有 Python 启动器故障单独记录，不冒充通过。
