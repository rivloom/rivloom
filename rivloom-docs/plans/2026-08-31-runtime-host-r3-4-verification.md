# Rivloom Runtime Host R3.4 验证记录

日期：2026-08-31。状态：后端已合入 `2b2d7915b3` 并独立复验；Gate R3 未通过。

## 基线和实施顺序

R3.3 #71–#74 已顺序普通 merge 至 `ab72fcf5ffe9436ae30393be7a78b0f37a6d1001`。
每次合并前复核原 Head、差异行数和反馈；未使用管理员绕过或 force-push。
独立主干 worktree 的 263 + 4 Rust、95 前端、TypeScript/Vite build、两组 Clippy
和桌面格式检查通过；原目录旧 main 及其 Cargo.toml 改动不动。
本轮 worktree 为 `C:/project/opencohive/.worktrees/r3-4-node-transport`，起自重新获取的 origin/main。

继续拆为每张少于 800 行的依赖 PR，完成各批测试和串行审查后再进入下一批：

1. [PR #75](https://github.com/rivloom/rivloom/pull/75)，545 行：成员可见数据投影及分页增量对账。
2. [PR #76](https://github.com/rivloom/rivloom/pull/76)，548 行：Node 原子对账、断线状态及重试。
3. [PR #77](https://github.com/rivloom/rivloom/pull/77)，738 行：TLS、证书身份固定和有界传输。
4. [PR #78](https://github.com/rivloom/rivloom/pull/78)，470 行：Windows Node 凭证保护。
5. [PR #79](https://github.com/rivloom/rivloom/pull/79)，694 行：Brain 认证服务、owner 鉴权和请求限流。
6. [PR #80](https://github.com/rivloom/rivloom/pull/80)，447 行：TLS 监听生命周期与有界 worker。
7. [PR #81](https://github.com/rivloom/rivloom/pull/81)，621 行：Node 客户端接线和两个 Node 的纵向传输测试。

#75–#81 均已按表中差异顺序普通 merge；原 Head 未变，无新增评论/审查反馈。
独立主干复验 305 + 4 Rust、95 前端、build、两组 Clippy 和桌面格式通过。
后续安全启动/托管工作见 [桌面托管记录](2026-08-31-r3-desktop-hosting-verification.md)。
发布时发现 fork 的 gh 默认指向上游，导致两次创建请求被拒绝；显式指定目标后已正常发布。
后续 gh 命令必须带 `-R rivloom/rivloom`，不能依赖工作目录推断仓库；未更换凭证或绕过访问控制。

## 对账和隐私边界

- 对账只发送独立 DTO，不序列化 BrainStore/BrainSnapshot、凭证 verifier、邀请或重放账本。
- 所有有效成员可见成员和 Node 的有界元数据；Task 内容目前仅对创建成员可见。
  owner 不因此获得其他成员的 Task；R4 有明确 Assignment 后才能增加对应参与者。
- 使用最后完整应用的 revision 拉取变更。记录包含最新状态，不是完整事件历史；没有删除，
  撤销用保留的记录表达。首轮 after=0；后续页面固定 at，Brain 变化时拒绝混用不同 revision。
- 每页最多一条记录，最多 192 条；控制帧上限 64 KiB，内嵌协议 v1 Message 仍最多 32 KiB。
  presence 超时产生新的修订号，但不推断 Task/Run 成败。
- 每页重新验证凭证有效期和撤销；返回错误不泄露底层存储、解析器或消息正文。

## 传输与验收要求

R3.4a：5 项新增对账行为测试通过，包括成员隔离、增量/修订冲突、逐页撤销/过期、
非法 cursor/帧和 presence 超时。`just test-rust` 268 + 4、`just check` 95 + build、
两组 Clippy（`-D warnings`）及桌面格式检查通过；上游格式化仍因既有 Python 启动器失败。
本批尚无监听端口或 Node 网络客户端。

R3.4b：Node 将完整分页结果一次性提交到本地有界视图（最多 192 条、3 MiB），
错序、身份/成员关联错误或 revision 改变均丢弃未完成批次，保留最后完成的 revision。
断线后的 running 视图保持 outcomeUnknown，重连本身不清除不确定性；不建立第二份 Run 权威。
最多保留一条显式确认可分享的 pending Message；重试只更新 revision，不变更幂等键或内容/hash。
确认必须匹配 pending key；确认后先对账。该 pending 目前仅在进程内跨重连保留，
R4 仍需从已有持久 Task/Run/回执恢复，不能重新生成执行键。回执队列不等于 Brain 已接受回执，
当前 Brain 继续拒绝 R4 尚未授权的 RunReceipt/Assignment，未接真实 Runtime 执行。
新增 7 项 Node 行为测试，`just test-rust` 275 + 4、95 前端 + build、两组 Clippy
及桌面格式通过；上游 `just fmt` 的既有 Python 故障未变。

R3.4c：使用 [Rustls 0.23.43](https://docs.rs/rustls/0.23.43/rustls/struct.ConnectionCommon.html)
和 ring，仅 TLS 1.3、固定 ALPN；正常证书链/名称/时间校验后额外核对可信 leaf DER SHA-256。
关闭 session resumption、tickets 和 early data，不使用环境变量密钥日志或自定义跳过校验器。
只接收明确配置的 loopback、RFC1918、Tailscale CGNAT 或 IPv6 ULA 地址，不解析任意 URL。
每次握手/帧使用总计 5 秒和 256 KiB wire 预算；先读 4 字节长度再分配最多 64 KiB，
无压缩、无明文降级，截断/超限/IO 错误关闭连接。接收缓冲清零，构造完成前不能发送应用数据。
6 项真实 Windows loopback TLS 测试覆盖最大帧、错误 root/name/pin/expiry/ALPN、明文对端、
截断/超限与预算。最终 281 + 4 Rust、95 前端 + build、两组 Clippy 和桌面格式检查通过。
测试证书生成器固定 rcgen 0.14.7；0.14.10 要求 Rust 1.88，未提升项目 MSRV。
Cargo.lock 只添加 TLS/测试所需依赖，没有升级既有包；所需 Bazel 锁更新仍因既有 Python
启动器失败，MODULE.bazel.lock 未变。此测试不等于私网两机或真实 Codex Gate 通过。

R3.4d：Node 凭证使用 Windows 的
[Credential Manager](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw)
generic credential、当前用户/本机持久作用域，Rivloom 不写普通明文 secret 文件。
槽名为完整、大小写敏感 binding 的 SHA-256，避免 Windows 不区分大小写造成身份碰撞；
读写仅限 Rivloom 命名空间和 1 KiB，恢复核对版本、binding、时间和 secret 格式。
已发现的槽不会覆盖，进程内写入串行；不宣称对其他同账号恶意进程的跨进程 CAS 防护。
没有凭证枚举、Runtime Token 读取、自动轮换或明文 fallback；非 Windows 原生后端明确不可用。
接收、序列化及 OS 读回缓冲清零，secret 的 Debug 脱敏。持久层和已建立 TLS 帧才允许显式 secret 字段。
新增 5 项测试；沙箱内原生写入失败，正常用户会话的完整 286 + 4 Rust 测试通过，
其中真实 OS 往返只创建随机命名合成槽并清理。95 前端 + build、两组 Clippy、桌面格式通过。
未把沙箱内失败或未运行的 Linux/macOS OS vault 记为通过；Bazel/上游格式化既有故障仍保留。

R3.4e：控制帧有独立 version、请求 correlation ID、严格封闭的操作/响应 schema，错误不回显正文。
TLS 接线必须先申请 session，再握手；最多 16 个 session，每 60 秒最多 64 次准入、1024 个请求，
每连接最多 256 个不同请求 ID，重复 ID 直接拒绝。业务 Submit 仍由 Brain 的持久幂等账本处理。
邀请兑换只能在新连接，随后必须认证；邀请/凭证只在整份存储提交后响应。
每次请求核对当前 credential；邀请创建/取消、成员撤销还必须核对真实 owner membership。
live TLS pulse 与重试的业务 Message 分开，不消耗业务重放账本；presence 仍按 30 秒 TTL 收敛。
冲突可先对账；其他拒绝、容量或存储错误关闭 session。存储锁不跨网络 IO 持有。
管理操作和邀请兑换不自动重试：回应丢失/OS 保存失败时，由 owner 检查并撤销孤立成员/重发邀请，
不重新兑换已消费的 secret，也不宣称跨存储与网络的 exactly-once 响应。
新增 7 项行为测试，293 + 4 Rust、95 前端 + build、两组 Clippy 和桌面格式通过。

R3.4f：仅显式启动的私网 TLS listener，准入发生在握手前，最多 16 个 worker。
stop/drop 先关闭所有 socket，再 join listener/worker，唤醒阻塞的握手和读帧；无遗留后台线程。
Windows loopback 测试暴露 accepted socket 继承非阻塞模式的问题，已在 TLS 握手前恢复阻塞模式。
新增 5 项测试覆盖真实 TLS 认证/邀请/对账、畸形帧隔离、停止和 permit 回收。
最终 298 + 4 Rust、95 前端 + build、两组 Clippy 和桌面格式通过；失败回归日志另行保留。
5 秒空闲读帧也会断开，调用方需及时 pulse 或显式重连；没有隐藏的自动重试。
尚未注册 Tauri 命令、桌面 UI 或自动监听服务，不更改防火墙。

R3.4g：Node 客户端串起 pinned TLS、OS vault 接口、认证和有界完整对账；最多读取 192 页。
加入后必须先保存凭证，随后认证和对账全部成功才返回健康客户端。
网络、协议、对账错误及 revision 冲突均断开连接，清除 readiness/未完成批次，保留完整视图与 pending。
显式重连从最后完成 revision 对账，显式重发只更新 revision，不改已确认业务键或 payload/hash。
owner 可创建/取消邀请及撤销成员；管理操作不自动重试。所有 IO 同步且有界，未来桌面托管必须移出 UI 线程。
每个连接仍受 256 请求 ID 上限及空闲读帧超时约束；无自动重连、后台心跳或任务执行。
新增 7 项真实 Windows loopback TLS 客户端测试，覆盖两个 Node 的邀请/认证/能力声明、双向 Task 隔离、
丢弃业务确认后的原键重发、撤销后的旧/新连接拒绝、listener 停止、vault 保存失败的孤立成员恢复、
取消邀请/普通成员越权及 revision 冲突后的显式重连。最终 305 + 4 Rust、95 前端 + build、
两组 Clippy 和桌面格式检查通过。
传输测试放在私有模块 `server_tests.rs` / `client_tests.rs`，使用真实 TCP/TLS、临时 BrainStore；
没有为原计划的独立 integration test 文件扩大公开 crate API。客户端组合测试使用内存 vault，
Windows 原生 vault 另有合成槽往返测试；丢失确认以丢弃已返回的应用确认模拟，不冒充跨机器断网。

后端完成不等于桌面功能已可用：仍需显式桌面托管/命令、安全提供服务端证书私钥及离线可信 root/name/pin、
凭证失效后的 owner 恢复流程和两台真实 Windows 设备验收。服务器 TLS 私钥目前由调用方注入，
没有声称已实现生产证书签发、轮换、分发或私钥持久化。上述接线和 Gate R3 应先于 R4，不能提前开始远端执行。

首版统一使用应用层 TLS（即使运行于 Tailscale），避免把私网 IP 当作加密/身份认证证明。
根证书、服务端名称和预先可信获取的证书 pin 均需校验；不做首次连接自动信任，失败不降级明文。
不自动更改 Windows 防火墙、安装私网软件或开启公开端口。
连接能力不等于 Runtime 可执行，不因重连自动 start Run；共享回执必须保留已确认内容和 hash。
两台真实 Windows 设备的 Gate R3 仍须独立验收，本机 TLS/测试 Node 不能替代。

`R2-FU1` elevated 多 Home 共存和真实执行/取消、边界/cleanup 继续延期，Gate R4 前必须补齐。
产品仍保持 `on-request + auto_review`；不修改 codex-rs、不恢复 CI、不处理 #37/#38，
不引入第二 Runtime、Marketplace 或 Skill Directory，不创建子 agent。

仓库级 `just fmt` 和依赖变化时所需 `just bazel-lock-update` 的既有 Python 启动器故障
与当前功能验证分开记录，不冒充通过；不修改全局环境来绕过。
