# Rivloom Desktop 浏览器唯一登录设计

日期：2026-08-28  
状态：已批准

## 背景

Rivloom Desktop 的 A1 账号接入原本同时提供 ChatGPT 浏览器登录和设备码登录。真实验证发现，设备码登录需要用户先在 ChatGPT 设置中启用，并且官方将其定位为主要面向无图形界面设备的 Beta 能力。桌面首发同时暴露两条路径会让用户在普通浏览器账号选择页和设备码验证页之间产生误解。

因此，Rivloom 首发只提供浏览器登录。上游 Codex App Server 的设备码协议保持不变，本次只移除 Rivloom 自己的设备码产品入口和实现。

参考：[OpenAI Authentication](https://learn.chatgpt.com/docs/auth)

## 用户体验

- 未登录时只显示“使用浏览器登录”。
- 启动后由系统浏览器打开官方 ChatGPT 登录页。
- Rivloom 显示“等待浏览器完成登录”和“取消登录”。
- 登录完成后，App Server 通知 Rivloom 刷新为已登录状态。
- 用户可取消待完成的浏览器登录，也可在已登录状态退出 Rivloom 的独立账号会话。
- 浏览器无法打开时显示可重试错误，不再建议切换设备码。

## 状态与边界

React 与 Tauri 之间只暴露以下账号状态：

```text
checking | signedOut | browserPending | signedIn | error
```

Rivloom 只注册以下账号命令：

```text
get_account_status
start_chatgpt_login
cancel_account_login
logout_account
```

删除 `devicePending`、`start_device_code_login`、`open_device_verification` 以及复制设备码相关 UI。

App Server 的 `account/login/start` 仍是外部协议边界。若浏览器登录请求意外返回 `chatgptDeviceCode`，Rivloom 不向 WebView 暴露设备码或验证 URL；如果响应包含可取消的 `loginId`，则先取消该尝试，再返回通用的可重试登录错误。

## 安全与隐私

- OAuth URL 只在 Rust 侧校验并交给系统浏览器，不进入 React 状态。
- Token、完整 OAuth URL、`loginId`、设备码和原始账号响应不得写入日志、文档或 Git。
- 退出只清除 Rivloom 独立 App Server 数据目录中的当前账号凭据，不影响浏览器中的 ChatGPT 会话。
- 本次不修改上游 `codex-rs` 的认证能力。

## 验证

- React 测试确认只有浏览器登录入口，等待态只有取消操作。
- Bridge 测试确认只调用四个账号命令。
- Rust 测试确认命令枚举、状态序列化和服务生命周期不再包含设备码。
- 保留浏览器请求意外收到设备码响应时的安全取消测试。
- 运行前端测试、构建、格式检查，以及 Tauri Rust 格式、测试和检查。
- 启动 Rivloom，真实回归浏览器登录启动和取消；不记录敏感认证数据。
