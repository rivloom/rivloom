# Gate R3：安全启动和桌面托管验证

日期：2026-08-31。状态：实施中；真实两机 Gate R3 未通过，不开始 R4。

## 基线与范围

R3.4 #75–#81 已按原 Head 顺序普通 merge，主干为 `2b2d7915b3b3c7493747f641d2cf68ecdbf8131e`。
其文件树与已验证的 `eaf5f07f6d` 完全相同。独立主干 worktree 的 305 + 4 Rust、95 前端、
TypeScript/Vite、两组 Clippy 和桌面格式检查通过。原主目录旧 main/Cargo.toml 改动保持不动。

本轮从最新 origin/main 新建 `C:/project/opencohive/.worktrees/r3-desktop-hosting`。
分批实现并逐批验证，每张 PR 小于 800 行：

1. 可导出/检查的公开信任资料，以及独立指纹确认后的 peer 构造。
2. 服务端 TLS 身份生成、OS 保护及严格恢复。
3. 使用受管本机目录和已保护身份，显式启动/停止 Brain 的桌面托管层。

不自动启动网络服务，不配置防火墙或安装 Tailscale，不读取 Runtime 登录凭据。
桌面加入/邀请 UI、凭证失效恢复及两机手工验收仍需独立完成；不把本机测试当 Gate 通过。

## 信任资料

- `TrustDescriptor` 是可检查但未获信任的公开 DTO；不包含邀请、Node secret 或 TLS 私钥。
- 严格版本与未知/重复字段校验，编码最多 8 KiB，证书最多 1 KiB，名称最多 253 字节。
  当前面向本机生成的紧凑证书，不宣称支持任意企业证书链。
- 仅明确私网/loopback IP 和非零端口，不接受 URL、DNS endpoint、通配监听或公开 IP。
- `TrustedPeer::confirm` 必须收到经独立可信渠道核对的完整、小写 SHA-256 leaf 指纹。
  导入资料不能自动产生信任；调用方不得拿资料自己算出的指纹冒充用户确认。
- 指纹确认只锚定 TLS 服务端身份，不授予 Brain membership、owner 角色或执行权限。
  实际连接仍做 TLS 证书链/名称/时间/pin 校验，邀请及 Node 凭证仍须另行认证。
- 资料的名称/IP/Brain ID 是待核对提示，不是身份证明；不自动发现服务或更换已确认的 pin。

首批新增 5 项测试覆盖显式确认、证书替换、畸形/超限资料、公开字段隔离及真实 Windows
loopback TLS 往返。310 + 4 Rust、95 前端 + build、两组 Clippy 和桌面格式检查通过。
仓库级 `just fmt` 仍因既有 Python 启动器故障失败；不计为通过，不修改 codex-rs。

## 必须保留的后续 Gate

- 两台真实 Windows 设备经受支持私网加入同一 Brain，并检查数据边界。
- 明确的证书信任引导、启动/停止和凭证失效处理 UI；不得自动确认指纹或扩大权限。
- R2-FU1 elevated 多 Home 共存与真实执行/取消验收继续延期，Gate R4 前必须补齐。
- 产品保持 `on-request + auto_review`；不恢复 CI，不处理 #37/#38，不接第二 Runtime、Marketplace 或 Skill Directory。
