# Gate R3：安全启动和桌面托管验证

日期：2026-08-31。状态：#82–#85 已合入 `a6dd8c7c05` 并独立复验；Gate R3 未通过。
后续 Node 接线见 [Node 桌面记录](2026-08-31-r3-node-desktop-verification.md)。

## 基线与范围

R3.4 #75–#81 已按原 Head 顺序普通 merge，主干为 `2b2d7915b3b3c7493747f641d2cf68ecdbf8131e`。
其文件树与已验证的 `eaf5f07f6d` 完全相同。独立主干 worktree 的 305 + 4 Rust、95 前端、
TypeScript/Vite、两组 Clippy 和桌面格式检查通过。原主目录旧 main/Cargo.toml 改动保持不动。

本轮从最新 origin/main 新建 `C:/project/opencohive/.worktrees/r3-desktop-hosting`。
分批实现并逐批验证，每张 PR 小于 800 行：

1. [PR #82](https://github.com/rivloom/rivloom/pull/82)，326 行：公开信任资料及独立指纹确认。
2. [PR #83](https://github.com/rivloom/rivloom/pull/83)，484 行：服务端 TLS 身份生成、OS 保护及严格恢复。
3. [PR #84](https://github.com/rivloom/rivloom/pull/84)，566 行：本机 Brain 初始化和显式生命周期。
4. [PR #85](https://github.com/rivloom/rivloom/pull/85)，406 行：桌面 managed state、四个 Tauri 命令与应用退出关闭接线。

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

## 受保护的服务端 TLS 身份

- 本机显式生成 P-256 自签名 ServerAuth 证书，签发起 30 天有效，notBefore 回溯 5 分钟。
  使用已锁定 rcgen 0.14.7/ring，不设计新的密码学协议；启用 rcgen 的 zeroize 并包装其序列化密钥。
- 单个 `Rivloom/brain-tls/v1/<Brain ID SHA-256>` OS 槽保存版本、Brain/name、时间、证书和私钥。
  使用 [Windows generic credential](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw)，
  当前用户/本机作用域，上限 2560 字节。Node 文档自身仍保持 1 KiB 上限及独立命名空间。
- 私钥、编码和读回缓冲清零；没有私钥 Debug/公开 DTO 或普通文件保存入口。
- 已存在槽拒绝创建，不自动轮换 pin；只允许 endpoint 变化时复用同一证书，仍须核对新地址。
- 恢复核对版本、Brain、时间、大小和密钥格式，用 Rustls 正常 verifier 校验证书名称/时间，
  并由 Rustls 检查证书/私钥匹配。不实现跳过验证器或明文 fallback。
- 非 Windows 原生后端仍明确不可用；不宣称防御同 Windows 账号恶意进程或实现跨进程 CAS。
- 新增 5 项测试，315 + 4 Rust、95 前端 + build、两组 Clippy 及桌面格式通过。
  原生测试只创建随机命名的合成 TLS 身份槽，结束清理；客户端实际证书校验通过真实 loopback TLS 验证。
- rcgen 从测试依赖移到生产依赖；base64/time 使用已有锁定版本，Cargo.lock 未增加或升级包。
  必须执行的 Bazel 锁更新与上游格式化仍被既有 Python 启动器故障阻塞，MODULE.bazel.lock 未变。

## 本机托管与桌面命令

本机托管核心：显式 initialize 只写入全新 app-owned 目录；先提交 Brain，再保护 TLS/Node 凭据，
最后以 create-new + sync 写入最多 12 KiB 的公开登记。没有跨文件/OS vault 的原子事务保证；
中途失败保留现场并报告 Incomplete/错误，重复初始化拒绝覆盖，不删除既有 Brain 或自动重发凭据。
登记缺失/损坏、凭据过期/丢失、设备或 owner 身份不匹配、证书变更均不能启动监听。
start 使用 R1 本机 Identity、已登记私网地址和同一证书；stop 回收 listener/worker，
正常重启仍使用同一 Brain 与 pin。initialize 和 app 启动都不自动监听；端口占用不会误报 Running。
操作使用 try-lock 返回 Busy；应用退出等待当前操作完成，再关闭服务并阻止排队命令重新启动。
核心新增 6 项行为测试，321 + 4 Rust、95 前端 + build、两组 Clippy、桌面格式通过。
组合测试使用内存 vault 和真实 TCP/TLS；原生 vault 在上一批独立验证。核心 PR 不含桌面命令接线。
Linux/macOS 未运行原生验收；修正 Windows 专用测试导入的 cfg，不将本机通过外推到其他平台。

桌面接线现已注册四个命令，且只允许 main 窗口调用：

| 命令 | 参数 | 成功结果 |
| --- | --- | --- |
| get_local_brain_status | 无 | notConfigured / stopped / running / faulted；stopped/running 带公开 profile |
| initialize_local_brain | params: {address, serverName} | 公开 HostProfile，不自动启动 |
| start_local_brain | 无 | running 与同一 profile |
| stop_local_brain | 无 | null；回收监听器和 worker |

profile 含 version、binding、descriptor、credentialExpiresAt，没有 Node secret/私钥。
结构化错误只有 invalid / notConfigured / incomplete / existing / busy / storage / credential / unavailable。
前端不得把 stopped 或本机测试当作两机 Gate 通过；credential/incomplete 不允许自动重新初始化。
命令内磁盘、OS vault 和 TLS 工作移至 spawn_blocking。路径固定为 app_local_data_dir 下
collaboration/brain-host，身份由 managed IdentityService 读取；不接受 IPC 传入路径/身份/密钥。
setup 只注册状态，不创建 Brain 或监听；ExitRequested/Exit 关闭服务，并阻止排队命令重新启动。
新增 4 项真实 Tauri invoke handler 测试，使用 MockRuntime、临时 IdentityStore、内存 vault；
未提供 AppServerState 的命令测试仍可完成初始化/启动/停止，未调用 Codex Runtime。
最终 `just test-rust` 为 321 + 4 项 project 包装测试 + 4 项 collaboration 包装测试；
95 前端 + build、普通与 test-tauri-commands 测试配置 Clippy（-D warnings）、桌面格式通过。
该变更没有前端组件，因此未产生 UI snapshot 变更；桌面交互/视觉验收尚未运行。

## 未完成的 Gate

- 两台真实 Windows 设备经受支持私网加入同一 Brain，并检查数据边界。
- Node 加入/连接/邀请接线已在后续 #86–#89 实现待合并；信任核对、启动/停止、凭证失效和初始化失败恢复 UI 仍缺。
  信任确认 API 不能代替用户真实核对；不得自动确认指纹或扩大权限。
- R2-FU1 elevated 多 Home 共存与真实执行/取消验收继续延期，Gate R4 前必须补齐。
- 产品保持 `on-request + auto_review`；不恢复 CI，不处理 #37/#38，不接第二 Runtime、Marketplace 或 Skill Directory。
