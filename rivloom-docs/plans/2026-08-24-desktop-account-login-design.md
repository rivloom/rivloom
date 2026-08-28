# Rivloom Desktop 账号登录设计

> 2026-08-28 更新：其中设备码产品路径已被[浏览器唯一登录设计](2026-08-28-desktop-browser-only-login-design.md)取代；本文保留为历史设计记录。

- 状态：已确认（阶段二 A1）
- 日期：2026-08-24
- 目标分支：`codex/desktop-account-login`
- 首发平台：Windows 10/11 x64

## 1. 目标

在桌面空壳和 App Server 初始化握手之上，建立后续会话、流式事件和审批可以共同复用
的长期 JSONL 连接，并实现 ChatGPT 托管登录的完整生命周期：状态查询、浏览器 OAuth、
设备码回退、取消和退出。本阶段不创建会话、不发送模型请求、不消耗模型额度。

## 2. 范围

### 2.1 包含

- 长期请求、响应、通知和服务端请求路由。
- 唯一请求 ID、乱序响应、请求超时、断开清理和有界输入。
- `account/read`、`account/login/start`、`account/login/cancel`、`account/logout`。
- `account/login/completed`、`account/updated` 通知。
- 浏览器 OAuth、设备码、取消、账号恢复和退出界面。
- URL 校验、日志脱敏、窄 Tauri 命令和前端 DTO。

### 2.2 不包含

- API Key、Bedrock、外部 Token、多账号或组织切换。
- 额度、用量、项目、会话、聊天、工具、审批、Diff、代理或自动重启。
- 官方 Codex 数据导入、云同步、多人或 AI 协作。
- 对 `codex-rs` 或 App Server v2 增加 Rivloom 专用扩展。

## 3. 当前缺口

第一阶段的 `AppServerSupervisor` 只在初始化时读取一条响应，随后保存子进程句柄；没有
持续读取 stdout，也没有请求 ID 到等待调用者的映射。账号登录同时依赖请求响应与服务
端通知，因此必须先建立长期连接，同时保留独立 `CODEX_HOME`、进程清理和 React 无
shell 权限等既有边界。

## 4. 架构

```text
React AccountAccessCard / useAccountStatus
                  │ 固定 Tauri 命令和归一化状态事件
                  ▼
Rust AccountService
  ├─ 账号状态、当前登录尝试、URL 校验和浏览器打开
  └─ App Server 账号协议适配
                  │ typed request / notification
                  ▼
AppServerConnection
  ├─ 请求 ID、有界 pending 表、超时和断开清理
  └─ JSONL 分帧及响应/通知/服务端请求分类
                  │
AppServerSupervisor / ProcessTransport
                  │ stdio JSONL
                  ▼
codex-app-server（OAuth 回调、凭据保存与刷新）
```

### 4.1 ProcessTransport

把 Tauri sidecar 适配器从已超过 400 行的 `process.rs` 提取到新模块。它负责启动唯一
sidecar、设置 `CODEX_HOME`、写入/终止控制、接收进程事件、stderr 脱敏和字节级 JSONL
分帧。stdout 不能假设一次事件正好是一整行。

### 4.2 AppServerConnection

- 初始化继续使用 ID `0`；普通请求从 `1` 开始递增。
- pending 请求先登记再写入，响应允许乱序。
- 通知交给 Rust 观察者，不直接转发给 React。
- A1 对不支持的服务端请求返回方法不支持错误，为后续审批保留分类入口。
- 写入失败、超时或断开都删除 pending；断开一次性失败全部等待者。

连接句柄可以克隆，但进程控制、pending 表和 reader 都留在 Rust。Tauri 命令等待 RPC
时不得持有 supervisor 生命周期锁。

### 4.3 AccountService

- `account/read` 是账号状态的唯一事实来源。
- Rust 保存当前 `loginId`、方式、验证 URL 和临时用户码。
- 只处理与当前 `loginId` 匹配的完成通知；过期通知忽略。
- 完成、取消、失败或断开后清理临时状态。
- 登录完成或退出后重新执行 `account/read`，不根据通知猜测最终状态。
- 只发出归一化的 `account-status-changed`。

### 4.4 Tauri 与 React 边界

固定命令为：

```text
get_account_status
start_chatgpt_login
start_device_code_login
cancel_account_login
logout_account
open_device_verification
```

`open_device_verification` 不接收 URL；Rust 验证并保存 URL 后使用现有
`tauri-plugin-shell` Rust API 打开系统浏览器。WebView 不增加 shell/open 权限，也不
获得任意 App Server 方法、JSON、路径或环境变量入口。

本阶段不引入路由库。现有概览页在核心服务卡片后加入账号卡片，核心服务未连接时禁用
账号操作。

## 5. 状态模型

```ts
export type AccountStatus =
  | { state: "checking" }
  | { state: "signedOut" }
  | { state: "browserPending" }
  | { state: "devicePending"; verificationUrl: string; userCode: string }
  | { state: "signedIn"; email: string | null; planType: string }
  | { state: "error"; message: string; retryable: boolean };
```

如果独立 `CODEX_HOME` 出现不支持的登录类型，映射为不可重试的安全错误，不能伪装为
ChatGPT 已登录。

## 6. 用户流程

### 6.1 初始读取

核心服务连接后，前端先订阅账号事件再调用 `get_account_status`。Rust 使用
`account/read` 和 `refreshToken: false`；只有空账号且 `requiresOpenaiAuth: true` 映射为
`signedOut`，空账号但不需要 OpenAI 认证则映射为不支持的配置错误。事件与初始读取竞态
时，新事件不能被旧响应覆盖。重连后重新读取。

### 6.2 浏览器 OAuth

1. 用户主动点击登录。
2. Rust 使用 `type: "chatgpt"` 发起登录，并请求托管 ChatGPT 成功页。
3. Rust 保存 `loginId`，验证 `authUrl`，打开系统浏览器并进入 `browserPending`。
4. 用户可取消或切换设备码。
5. 匹配的完成通知到达后，清理尝试并重新读取账号。

### 6.3 设备码与退出

切换方式前先取消当前尝试。设备码响应只向前端暴露验证地址和临时用户码，并提供复制、
打开和取消。退出登录先显示可访问的确认对话框；失败时保留已登录状态，不能伪造成功。

## 7. 安全与资源边界

- Token 由 App Server 保存在 Rivloom 独立 `CODEX_HOME`，不进入 React 或普通日志。
- 不记录完整 OAuth URL、查询参数、`loginId`、设备码或原始账号响应。
- 使用 `tauri::Url` 解析；只允许 HTTPS 官方认证主机及真实子域，禁止 URL 凭据和非
  默认端口。
- 单条未完成 JSONL 最大 4 MiB；pending 最多 64；普通 RPC 默认超时 10 秒。
- OAuth 用户等待不设短超时，但始终可以取消。
- 无效 UTF-8/JSON、重复或未知响应 ID 只产生有界脱敏诊断，不 panic。
- 用户操作后 100 ms 内显示本地等待反馈，等待不能阻塞 Tauri 主线程或 React。

## 8. 故障与恢复

| 故障                      | 行为                                         |
| ------------------------- | -------------------------------------------- |
| 账号读取失败              | 显示暂不可用，允许重试，不推断为未登录       |
| 浏览器 URL 无效或无法打开 | 取消当前尝试并建议设备码                     |
| 过期通知                  | 忽略                                         |
| App Server 退出           | 失败 pending、清理登录尝试、进入核心服务错误 |
| 手动重连                  | 建立新连接并重新读取账号                     |
| 退出失败                  | 保持已登录并显示错误                         |

A1 不增加自动重启；继续使用第一阶段手动重试。

## 9. 关键决策与取舍

| 决策                                                                                  | 原因                                                                                             | 代价/未采用方案                         |
| ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ | --------------------------------------- |
| 通用长期连接                                                                          | A1 已需响应+通知，B 还需流式事件和审批                                                           | 比账号专用读循环改动大，但避免 B 再推翻 |
| Rust 打开浏览器                                                                       | React 不获得任意 URL/shell 能力                                                                  | 需可测试 opener 和严格 URL 兼容策略     |
| App Server 管凭据                                                                     | 复用 OAuth、保存和刷新                                                                           | 账号能力受配套协议版本约束              |
| A1.1 后按 A1.2a-1/A1.2a-2a1/A1.2a-2a2a/A1.2a-2a2b/A1.2a-2a3/A1.2a-2b/A1.2b/A1.2c 交付 | 分开审查读取核心、URL/协议安全、浏览器启动、并发生命周期、设备码、通知与账号操作、桥接和界面风险 | 八个独立 PR，按顺序合并后再开始下一段   |

## 10. 测试与验收

- Rust：JSONL 分帧、消息分类、乱序响应、通知、超时、断开、URL 和账号状态机。
- React：Bridge、竞态 Hook、六种状态、设备码、取消和退出确认。
- 视觉：所有状态在 1180×760 和 960×640 下无溢出并支持键盘。
- 原生：fake sidecar 验证异常；真实浏览器和设备码登录需用户单独批准并参与。
- 安全：无 Token/完整 OAuth URL/设备码进入 WebView、日志或 Git。
- 回归：第一阶段启动、重试、退出清理和全部自动化检查继续通过。
- 全程无 `thread/start`、`turn/start` 或模型调用。

## 11. 文件结构

```text
apps/desktop/src/{components/AccountAccessCard,hooks,lib,types}
apps/desktop/src-tauri/src/account/{login,mod,service,types}.rs
apps/desktop/src-tauri/src/app_server/{connection,process,protocol,transport,wire}.rs
```

新测试放在显式 sibling `*_tests.rs`；新实现模块目标低于 500 行，`process.rs` 应缩小。

## 12. 参考资料

- [总体架构](2026-08-24-rivloom-desktop-architecture-design.md)
- [桌面空壳设计](2026-08-24-desktop-shell-design.md)
- [OpenAI Codex App Server](https://learn.chatgpt.com/docs/app-server)
- `codex-rs/app-server-protocol/src/protocol/v2/account.rs`
- `codex-rs/app-server-protocol/schema/json/v2/`
