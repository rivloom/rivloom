# Gate R3：Node 桌面接线验证

日期：2026-08-31。状态：分阶段实现；Gate R3 未通过。

## 已合并基线

托管/信任 #82–#85 已按原 Head 普通 merge，最新主干为 `a6dd8c7c054d44c42bfaf44be0fd0118adf43b94`。
其文件树等于已验证的 `8214c244de`。独立 `r3-hosting-main-verification` worktree 的
321 + 4 + 4 Rust、95 前端、TypeScript/Vite、普通及 feature 测试配置 Clippy、桌面格式检查通过。
本轮从该 origin/main 新建 `C:/project/opencohive/.worktrees/r3-node-desktop`，不使用旧本地主干。

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

## 剩余工作与保留限制

Node service、连接/加入/邀请命令与 UI 分阶段接线。所有网络操作显式触发，不自动确认信任、重新加入、
重试管理操作或调用 Runtime。邀请 UI 需要短时、一次性呈现 secret，禁止写入持久化状态、日志和错误。
TLS/Node 凭证过期与不完整初始化的恢复流程、真实两 Windows 设备验收仍未完成。

R2-FU1 elevated 多 Home 共存与真实执行/取消/边界/cleanup 验收继续延期，在 Gate R4 前必须补齐。
不修改 codex-rs，不恢复 CI，不处理 #37/#38，不进入 R4/第二 Runtime/Marketplace/Skill Directory。
仓库级 `just fmt` 仍受既有 Python 启动器故障影响，不计为通过。
