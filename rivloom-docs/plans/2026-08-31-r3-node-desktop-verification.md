# Gate R3：Node 桌面接线验证

日期：2026-08-31。状态：Node 登记、会话、邀请及桌面命令 #86–#89 已合并；Gate R3 未通过。

合并收尾：最新主干 `8b2190bfa6996cba654fdeeb54ae237270b7d883` 的文件树等于已验证的 #89 Head。
独立 `r3-node-main-verification` worktree 的 334 + 4 + 4 + 4 Rust、95 前端、
TypeScript/Vite、普通及 feature 测试配置 Clippy、桌面 Rust 格式检查通过。
此前一次原生合成凭证读取异常仍保留，未宣称根因已解决。

## 已合并基线

托管/信任 #82–#85 已按原 Head 普通 merge，最新主干为 `a6dd8c7c054d44c42bfaf44be0fd0118adf43b94`。
其文件树等于已验证的 `8214c244de`。独立 `r3-hosting-main-verification` worktree 的
321 + 4 + 4 Rust、95 前端、TypeScript/Vite、普通及 feature 测试配置 Clippy、桌面格式检查通过。
本轮从该 origin/main 新建 `C:/project/opencohive/.worktrees/r3-node-desktop`，不使用旧本地主干。

分批 PR：[登记 #86](https://github.com/rivloom/rivloom/pull/86) 434 行；
[会话 #87](https://github.com/rivloom/rivloom/pull/87) 538 行；
[owner/邀请 #88](https://github.com/rivloom/rivloom/pull/88) 417 行；
[桌面命令 #89](https://github.com/rivloom/rivloom/pull/89) 569 行。

## 第一阶段：持久化 Node 登记

- 一个桌面受管目录对应一个 Node 登记，不接受 IPC 或网络指定路径。
- 创建前校验公开 descriptor、独立确认的指纹和 R1 本机 identity/device；先持久化登记，再允许后续 Join。
- 登记只含 version、identityId、deviceId、descriptor 和 confirmedFingerprint；邀请及 Node secret 不落盘。
- 另一个 create-new 文件在成功后保存完整 CredentialBinding。目录或文件已存在均不覆盖；没有删除/reset API。
- 无 binding 的登记意味着尝试未完成，后续须显式恢复；不能据此重新发送邀请兑换。
- 恢复核对已保存的指纹而不是重算后自动确认，并检查本机身份与 Brain/device 绑定。
- 读取上限为登记 12 KiB、绑定 1 KiB；拒绝非普通文件、符号链接、未知/重复字段、版本和格式错误。
  文件写入使用 create-new + sync，Unix 文件权限 0600。没有宣称抵抗同用户恶意篡改或跨文件事务。
- 四项行为测试覆盖中断/恢复、重复创建、身份/指纹/绑定不一致、畸形/超限及失败后保留现场。
- 325 + 4 + 4 Rust、95 前端及构建、普通与 feature 测试配置 Clippy、桌面格式检查通过。
- 默认 `pnpm format` 在独立主干和该分支均报告相同的 75 个既有文件；未做无关整树格式化，未计为通过。

## 第二阶段：显式 Node 会话

加入前落盘信任登记；Join 结果、vault 写入及初次认证对账全部成功后才提交 binding。
失败保留登记，缺少 binding 的状态为 recoveryRequired；不会自动重发 Join 或删除登记。
connect 从已有登记和 OS 凭证恢复同一 Node，refresh 显式 pulse + 对账，disconnect 保留最后完整修订号。
connected 表示最后一次认证/对账成功，不是持续在线保证；没有后台保活或自动重连。
本机身份变化、凭证失效、远端撤销、TLS/传输错误均返回有界错误，不回显 secret。
退出等待正在运行的同步操作，关闭通道并阻止排队操作重新连接。没有 Task/Run 入口。
五项真实 TLS 组合测试覆盖正常加入/重启、撤销、网络失败、远端加入后 vault 失败及 busy/退出保护。

默认前端格式检查的 75 项提示在主干同样出现；`pnpm format --end-of-line auto` 通过，确认是 checkout
换行差异，未修改既有前端文件。完整测试首轮原生合成 TLS 槽在成功恢复后的一次 read 返回不存在，
保留 `.r3-session-rust.log`；未弱化断言或加入测试重试逻辑。原样复跑 330 + 4 + 4 Rust 通过，
见 `.r3-session-rust-retry.log`；根因尚未确定，真实设备验收须关注凭证持久性。
95 前端及构建、两组 Clippy、桌面格式检查通过。

## 第三阶段：owner 接入、成员目录及邀请

本机 owner 使用 managed Brain profile、明确确认的 fingerprint 和已有 OS 凭证连接；认证后的
成员必须是未撤销 owner 且匹配 R1 identity，才写入 Node 登记。不会为 owner 自动兑换邀请或创建新成员。
成员目录只返回最多 128 个 member/node 摘要、最多 64 KiB，不包含 Task、完整 announcement、identity/device
ID、路径或密钥。目录是最后完整对账结果，disconnected 时不作为当前目录返回。
创建、取消邀请和撤销均调用已认证 Client，权限仍由 Brain 检查；不自动重试管理操作。
专用 InvitationSecret 允许短时 IPC 传递邀请，Debug 脱敏、解析缓冲清零；Node/TLS secret 没有 DTO 出口。
邀请只含 brainId、invitationId、expiresAt 和 secret，不进入状态、目录或普通文件。
四项真实 TLS/DTO 测试及完整 334 + 4 + 4 Rust、95 前端及构建、两组 Clippy、桌面格式通过。
本阶段原生凭证测试通过；此前一次消失异常仍保留，未宣称原因已解决。
审查追加保护：owner 登记已存在时，在建立连接前拒绝；已停止的 Brain 也不会触发重复认证。

## 第四阶段：桌面命令

应用 setup 仅登记 managed Node state，路径固定在 app_local_data_dir/collaboration/node-client。
所有命令只接受 main 窗口，磁盘/vault/TLS 工作运行在 spawn_blocking；退出先关闭 Node，再关闭 Brain。
身份只来自 R1 IdentityService，不接受调用方提供身份、目录、凭证绑定或私钥；不依赖 AppServerState。

| 命令 | 参数 | 成功结果 |
| --- | --- | --- |
| get_node_status | 无 | state、registration、binding、revision |
| join_brain | params: {descriptor, confirmedFingerprint, invitation} | NodeStatus |
| connect_brain_owner | params: {confirmedFingerprint}；profile 取自 managed running Brain | NodeStatus |
| connect_brain / refresh_brain | 无 | NodeStatus |
| disconnect_brain | 无 | null |
| list_brain_members | 无 | revision、entries；最多 128 项 / 64 KiB |
| create_brain_invitation | 无 | brainId、invitationId、expiresAt、secret；仅短时展示/传递 |
| cancel_brain_invitation | params: {invitationId} | null |
| revoke_brain_member | params: {memberId} | null |

descriptor 是最多 8 KiB 的公开登记 JSON 字符串；invitation 与创建结果同形，拒绝未知字段、无效 code、
Brain 不匹配和已到期/剩余超过 10 分钟的邀请。信任指纹错误或邀请校验失败时不创建 Node 登记。
NodeStatus.state 为 notConfigured / recoveryRequired / disconnected / connected；registration 和 binding
在未配置时为 null，绑定尚未提交时仅有 registration。状态/目录没有邀请 secret、Node credential 或 TLS key。
序列化错误只有 invalid / notConfigured / recoveryRequired / existing / storage / busy / disconnected /
transport / credential / rejected / unavailable。UI 不得对 uncertain Join 或管理操作自动重试。
四项实际 Tauri invoke handler 测试使用两个 MockRuntime 应用、临时 IdentityStore、内存 vault 和真实 TLS，
覆盖全部命令、窗口/managed state 限制、未知/危险输入、不完整/损坏记录及退出后拒绝连接。
此批没有前端组件或视觉变化；未运行真实桌面交互或两 Windows 设备验收。
最终 `just test-rust` 为 334 Rust + 4 project 包装 + 4 hosting 包装 + 4 Node 包装测试通过；
95 前端、TypeScript/Vite、两组 Clippy（含 feature tests）、cargo check、桌面格式检查通过。
最终复验及上一轮成员阶段原生凭证测试均通过，先前单次异常仍保留在上述日志中。

## 剩余工作与保留限制

Node service 和连接/加入/邀请命令已接线，UI 仍待实现。所有网络操作显式触发，不自动确认信任、重新加入、
重试管理操作或调用 Runtime。邀请 UI 需要短时、一次性呈现 secret，禁止写入持久化状态、日志和错误。
TLS/Node 凭证过期与不完整初始化的恢复流程、真实两 Windows 设备验收仍未完成。
Runtime capability announcement 的桌面接线尚未暴露；此批不猜测或自动发布 Runtime 能力。
非 Windows 原生 vault 仍不可用，没有跨平台原生验收声明。

R2-FU1 elevated 多 Home 共存与真实执行/取消/边界/cleanup 验收继续延期，在 Gate R4 前必须补齐。
不修改 codex-rs，不恢复 CI，不处理 #37/#38，不进入 R4/第二 Runtime/Marketplace/Skill Directory。
仓库级 `just fmt` 仍受既有 Python 启动器故障影响，不计为通过。
