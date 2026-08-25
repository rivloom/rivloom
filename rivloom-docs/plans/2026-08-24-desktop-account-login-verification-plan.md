# Rivloom Desktop 账号登录验证方案

- 状态：实施前检查清单
- 日期：2026-08-24
- 分支：`codex/desktop-account-login`

## 1. 原则

- 先验证协议、状态机、Tauri 边界和假协议，再进行真实登录。
- 真实浏览器和设备码登录都需要用户参与，并在开始前再次明确批准。
- 不读取、记录、截图或提交 Token、完整 OAuth URL、`loginId`、设备码或账号文件。
- 不调用 `thread/start`、`turn/start` 或模型接口；临时截图、日志和二进制不进 Git。

## 2. 自动化矩阵

| 层 | 必须覆盖 |
|---|---|
| JSONL | 分块、多行、CRLF、非 UTF-8、4 MiB 上限、四类消息、无效结构 |
| Connection | 乱序响应、通知、10 秒超时、64 pending、写失败、断开、重复/未知 ID |
| Account | 需认证/无需认证的空账号、ChatGPT/不支持账号、两种登录、URL、通知、取消、退出 |
| Tauri | 六个固定命令、一个归一化事件、断开清理、能力文件不扩大 |
| React Bridge/Hook | 先监听后读取、竞态、重连、去重、卸载、统一错误 |
| UI | 六种状态、复制/打开/取消、错误、服务不可用、退出确认和键盘焦点 |

从 `apps/desktop` 运行：

```text
just fmt
just test
just check
just test-rust
just check-rust
```

从 `codex-rs` 运行 `just fmt`。再运行 `git diff --check`、`git status --short` 和
`git diff --stat 584ba0a7c4`。完整上游测试必须另行获批。

## 3. 视觉验证

用确定性 Tauri IPC mock 在 1180×760 和 960×640 渲染：checking、signedOut、
browserPending、devicePending、普通/超长 signedIn、error、logout confirmation。

逐张检查：

- 无页面横向溢出，底部状态栏可见，主区内部滚动。
- 设备码、长邮箱和套餐不撑破布局，完整值仍可获取。
- 状态不依赖颜色；按钮、禁用、焦点环和 live/alert 语义正确。
- 退出确认默认焦点在取消，Escape 和关闭后焦点恢复正确。
- reduced-motion 生效，控制台无警告或未处理 Promise。

截图仅保存到 `%TEMP%\rivloom-account-login-screenshots`。

## 4. 假协议原生验证

1. 初始化后 `account/read` 显示未登录。
2. 浏览器 opener 只记录测试 URL，不真实打开。
3. 完成与更新通知任意顺序都以再次读取为准。
4. 设备状态只显示测试网址/码；取消后旧通知无效。
5. 账号请求中进程退出会失败 pending、清理尝试并进入核心错误。
6. 手动重试建立新连接并重新读账号。
7. 关闭应用后没有 Rivloom/fake App Server 进程残留。

## 5. 真实登录、取消和退出

先向用户说明将打开官方 ChatGPT 页面、凭据由 Rivloom 独立 App Server 保存、不会调用
模型，并获得明确批准。

浏览器流程验证：未登录 → 打开一个校验后的 HTTPS 官方地址 → 用户完成登录 → 桌面
再次读取账号 → 重启后恢复。设备码流程需要单独批准，验证显示、复制、打开、完成后
立即清码及重启恢复。

分别验证两种登录取消，确保旧通知不覆盖新状态。退出时先显示确认；取消保持登录；确认
后再次读取为未登录，且不删除整个 `codex-home`、设置或历史。失败路径只用 fake
transport，不篡改真实账号文件。

## 6. 安全与仓库审查

- React 只能调用六个账号命令；`open_device_verification` 不接收 URL。
- capability 无 shell/open/spawn/execute；Rust 只允许合规 HTTPS 官方认证地址。
- DTO、日志、错误和 Git 中无 Token、完整 URL、loginId、设备码或原始账号响应。
- 单行、pending、RPC 和诊断都有硬上限；断开/退出清理请求、尝试和子进程。
- `git diff 812c27ffa9 -- codex-rs` 为空；没有二进制、target、dist、截图、日志或账号文件。
- 新模块低于 500 行；任一 A1.2 PR 接近 800 行时先提议更小的连贯拆分。
- `LICENSE` 继续是 Apache-2.0，没有不兼容资产或依赖。

## 7. 最终记录

实施后创建 `2026-08-24-desktop-account-login-verification.md`，记录精确命令及数量、逐张
视觉结论、fake/真实原生结果、版本、故障修复、已知限制、安全审查和无模型调用证据。
