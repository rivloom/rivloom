# Rivloom Runtime Host R3.4 验证记录

日期：2026-08-31。状态：实现中；Gate R3 未通过。

## 基线和实施顺序

R3.3 #71–#74 已顺序普通 merge 至 `ab72fcf5ffe9436ae30393be7a78b0f37a6d1001`。
每次合并前复核原 Head、差异行数和反馈；未使用管理员绕过或 force-push。
独立主干 worktree 的 263 + 4 Rust、95 前端、TypeScript/Vite build、两组 Clippy
和桌面格式检查通过；原目录旧 main 及其 Cargo.toml 改动不动。
本轮 worktree 为 `C:/project/opencohive/.worktrees/r3-4-node-transport`，起自重新获取的 origin/main。

继续拆为每张少于 800 行的依赖 PR，完成各批测试和串行审查后再进入下一批：

1. [PR #75](https://github.com/rivloom/rivloom/pull/75)，545 行：成员可见数据投影及分页增量对账。
2. [PR #76](https://github.com/rivloom/rivloom/pull/76)，548 行：Node 原子对账、断线状态及重试。
3. 受支持 TLS、证书身份固定和有界帧传输。
4. Node 凭证的本机保护，不落明文 secret。
5. Brain/Node 认证服务接线、限速和纵向传输测试。

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
