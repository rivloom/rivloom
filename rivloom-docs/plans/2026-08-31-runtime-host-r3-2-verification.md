# Rivloom Runtime Host R3.2 验证记录

日期：2026-08-31。状态：R3.2 本地核心实现并通过自动化验证，PR 待审查合并；Gate R3 未通过。

## 基线与拆分

从已完成主干复验的 `origin/main@4b4d5f7839` 创建独立 worktree
`C:/project/opencohive/.worktrees/r3-2-node-credentials`。主目录旧 main 和其原有
`apps/desktop/src-tauri/Cargo.toml` 改动保持不动。

- R3.2a：[Draft PR #69](https://github.com/rivloom/rivloom/pull/69)，
  `codex/r3-2-node-credentials` → main，`adea227c0b`，561 changed lines。
- R3.2b：`codex/r3-2-single-use-invitations` → `codex/r3-2-node-credentials`，
  短期一次性邀请与成员/Node 兑换。先合并 #69，再将此 PR 重定到 main；各自低于 800 行。

## 凭证边界

- 使用 OS 随机源生成 256-bit opaque bearer secret，解析只接受 64 位小写十六进制。
  没有随机源降级；不使用密码、身份 ID 或时间戳派生 secret。
- Brain 仅保留带用途域的 SHA-256 verifier；邀请与 Node 凭证不共享用途域。
  固定长度 verifier 使用常量时间比较；secret 的 Debug 固定脱敏，无 Serialize/Clone，
  临时随机缓冲区及拥有的字符串在 drop 时清零。调用方显式借出或复制的内容不在此保证内。
- 凭证固定绑定 Brain/member/Node/device，24 小时过期；每个 Node 只能签发一次。
  每个 registry 最多 64 条记录，撤销记录保留占位，防止旧身份恢复或无界增长。
  当前没有续期/轮换/清理产品入口；需由后续受信管理流程处理，不能静默复活旧凭证。
- `connect` 验证 secret 与完整绑定；`authorize_task` 每次查询同一 live registry。
  撤销成员同时禁用其所有 Node，已认证的旧 session 也无法接新任务；其他成员不受影响。
  session 不可反序列化，registry 不可 Clone；同名新 registry 的新凭证不接受旧 session。
- 所有 now 必须来自受信 Brain 时钟。时钟回退时拒绝访问，避免过期后复活；时间恢复前
  可能暂时拒绝合法访问。远端消息时间戳不得进入此接口。
- 签发与撤销是受信本地管理 API，不接受远端权限声明。这里的 task gate 只检查凭证；
  不授予 owner 角色，不代替 Task 归属校验、本地接受、worktree 或 Runtime 权限检查。

上述原语采用 [getrandom 系统随机源](https://docs.rs/getrandom/0.3.4/getrandom/fn.fill.html)、
[subtle 固定长度比较](https://docs.rs/subtle/2.6.1/subtle/trait.ConstantTimeEq.html) 和
[Zeroizing](https://docs.rs/zeroize/1.9.0/zeroize/struct.Zeroizing.html)，没有自创加密协议。
Bearer secret 被窃取后在撤销/过期前仍可使用；这些内存类型不提供硬件设备证明。

## 邀请与成员兑换

- 邀请固定绑定 Brain，10 分钟有效，最多 32 个 pending；到期边界拒绝兑换。
  创建时清理已过期条目，取消邀请立即移除其 verifier。时钟回退不会复活过期邀请。
- 邀请 ID 与 secret 分别生成；Brain 仅保留 verifier 与有效时间，不保留邀请明文。
  只有凭证签发成功后才消费邀请；错误 secret、错 Brain、非法输入或容量不足不会花掉邀请。
  同一邀请再次兑换被拒绝，不重复生成成员/Node 凭证。
- 兑换在 Brain 生成独立的 member/Node ID，绑定申请者提供的有界 identity/device ID，
  只返回 member 角色；接口没有客户端指定 member/Node ID 或 owner 角色的字段。
  identity ID、device ID 和昵称是自报标签，邀请持有证明不等于真实身份或设备证明。
- 邀请 secret 不能当 Node secret 使用。重新邀请产生新成员与新 Node；同一 identity 标签
  的新成员不复用已撤销成员，旧会话仍被拒绝。成员撤销不等于永久封禁人的自报 identity。
- core 的 `&mut` 串行兑换只保证内存操作顺序。R3.3 必须原子持久化邀请消费、成员和凭证，
  防止崩溃后重复使用；响应丢失不会重发明文 secret，须由管理者撤销孤立成员后重新邀请。
  开放 join endpoint 前需补身份核对/邀请安全交付，不能只凭网络地址或昵称批准加入。

一次性、限时、随机和安全保存的 token 生命周期参考
[OWASP token 指引](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html)；
这里是成员邀请，不声称实现了完整密码恢复流程或已验收网络认证。

## 自动化证据

首次回归使错误 secret、到期边界和撤销旧 session 三项测试失败，修复后通过。
新增覆盖身份字段替换、跨 registry 会话、容量/墓碑、输入上限、秘密脱敏和固定错误。
锁文件只新增 subtle 2.6.1、zeroize 1.9.0；getrandom 0.3.4 已在锁中，无无关依赖升级。
初次离线解析受缓存冲突影响；在沙箱外获取 registry 元数据后正常解析，未降低 TLS 校验。

R3.2b 首次回归使重复兑换、错误 proof 和过期邀请三项测试失败，修复后通过。
覆盖取消/回退、跨 Brain、secret 脱敏、输入与 pending 上限、签发失败保留邀请、
重新邀请与旧会话撤销，共新增 7 项邀请测试；R3.2 总计新增 14 项行为测试。

在 `apps/desktop` 分别验证：

| 检查 | R3.2a | R3.2b |
| --- | --- | --- |
| `just test-rust` | 231 + 4 | 238 + 4 |
| `just check` | 95 + TS/Vite build | 95 + TS/Vite build |
| Clippy `--tests -- -D warnings` | 通过 | 通过 |
| Clippy `--lib --features test-tauri-commands -- -D warnings` | 通过 | 通过 |
| 桌面格式检查、diff、本地文档链接与范围检查 | 通过 | 通过 |

最后通过桌面 `cargo fmt` / `--check` 和 `git diff --check`，未在格式化后重跑同一批测试。
测试没有调用真实模型；前端快照仅换行状态刷新，没有内容变化。
仓库要求的 `just bazel-lock-update` 和 `codex-rs` 下 `just fmt` 均已尝试，
但被既有失效 Python 启动器阻断，未生成 MODULE.bazel.lock 更新，两项不计为通过。

## 未完成的接线与 Gate

核心尚未注册 Tauri 命令、监听端口或发送网络数据，也未把 Node secret 写入磁盘。
R3.3 必须持久化 Brain 权威状态/撤销/时钟并验证恢复，不能通过重新建空 registry 恢复权限。
R3.4 必须把认证与每次准入接入连接/派发，并在撤销后关闭连接；这里没有声称已关闭真实 socket。
开放网络前还需 owner 管理鉴权、限速、安全凭证存储、秘密传输及断线验证。
R3.1 数据消息不能携带 secret；认证交换须独立走加密控制通道。
首版优先已加密私网；普通 LAN 必须经过审查的 TLS/证书固定，禁止明文凭证和任务。

`R2-FU1` 的 Windows elevated 多 Home 共存、真实执行/取消及边界/cleanup 验收继续延期，
Gate R4/Windows 可用性发布前必须补齐。当前仍为 `on-request + auto_review`。
没有修改 codex-rs、恢复 CI、处理 #37/#38、引入第二 Runtime/Marketplace/Skill Directory，
没有使用子 agent。

下一步：审查并顺序合并两张 R3.2 PR，主干复验后开始 R3.3 Brain 状态存储、presence 与修订号。
