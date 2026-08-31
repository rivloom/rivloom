# Rivloom Runtime Host R3.2 验证记录

日期：2026-08-31。状态：R3.2a 节点凭证核心已实现；邀请兑换进行中，Gate R3 未通过。

## 基线与拆分

从已完成主干复验的 `origin/main@4b4d5f7839` 创建独立 worktree
`C:/project/opencohive/.worktrees/r3-2-node-credentials`。主目录旧 main 和其原有
`apps/desktop/src-tauri/Cargo.toml` 改动保持不动。

- R3.2a：`codex/r3-2-node-credentials`，秘密类型、凭证签发、连接认证和撤销准入。
- R3.2b：后续独立 PR，短期一次性邀请与成员/Node 兑换；两个 PR 各自低于 800 行。

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

## 自动化证据

首次回归使错误 secret、到期边界和撤销旧 session 三项测试失败，修复后通过。
新增覆盖身份字段替换、跨 registry 会话、容量/墓碑、输入上限、秘密脱敏和固定错误。
锁文件只新增 subtle 2.6.1、zeroize 1.9.0；getrandom 0.3.4 已在锁中，无无关依赖升级。
初次离线解析受缓存冲突影响；在沙箱外获取 registry 元数据后正常解析，未降低 TLS 校验。

R3.2a 在 `apps/desktop` 通过 `just test-rust`（231 + 4）、`just check`（95 + build）、
普通 `--tests` 与 `--lib --features test-tauri-commands` 两组 Clippy（均 `-D warnings`）。
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
