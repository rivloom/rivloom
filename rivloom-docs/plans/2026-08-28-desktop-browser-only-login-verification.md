# Rivloom Desktop 浏览器唯一登录验证记录

- 日期：2026-08-28
- 分支：`codex/a1-browser-only-auth`
- 状态：自动化、静态验证和最终人工交互回归全部通过

## 范围

验证 Rivloom Desktop 已将 A1 账号接入收敛为浏览器唯一登录，并且没有修改上游 `codex-rs` 或 App Server 的设备码协议能力。

## 已通过

- React：8 个测试文件、37 个测试通过。
- Rust：101 个单元测试通过。
- 前端 TypeScript 与 Vite 生产构建通过。
- Rust `cargo clippy --all-targets -- -D warnings` 通过。
- 本次变更涉及的 React、CSS 和 Markdown 文件通过 Prettier 检查。
- 真实启动时，未登录账号卡片只显示“使用浏览器登录”，不再显示设备码入口。
- 源码检查确认 React 与 Tauri 中不存在 `devicePending`、`start_device_code_login`、`open_device_verification` 或对应用户文案。
- 浏览器登录意外收到 `chatgptDeviceCode` 响应时的安全取消和清理测试仍保留。

## 人工交互回归

2026-08-28，用户在 Rivloom 原生窗口中手工完成以下流程并反馈正常：

1. 点击“使用浏览器登录”。
2. 确认 Rivloom 显示“等待浏览器完成登录”和唯一的“取消登录”操作。
3. 点击“取消登录”。
4. 确认状态恢复为未登录，且仍只有浏览器登录入口。

回归结束后再次启动应用，核心服务显示已连接，账号显示未登录，页面仍只有浏览器登录
入口。随后已正常关闭 Rivloom 和本地 Vite 服务。

## 安全记录

验证期间没有读取、记录、截图或提交 Token、完整 OAuth URL、`loginId`、设备码、账号邮箱
或账号文件。人工交互回归也遵守了这一边界。
