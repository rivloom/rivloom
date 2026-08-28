# Rivloom Desktop 浏览器唯一登录实施计划

日期：2026-08-28  
设计：`2026-08-28-desktop-browser-only-login-design.md`

## 目标

将 A1 账号接入收敛为浏览器唯一登录，完整删除 Rivloom 的设备码入口、状态、Bridge、Tauri 命令与服务代码，同时保留外部协议异常响应的防御性处理。

## 实施步骤

1. 先更新 React 测试，固化未登录和浏览器等待态的唯一交互。
2. 删除 `AccountStatus.devicePending`、设备码 Bridge 方法和 Hook action。
3. 删除账号卡片中的设备码选择、切换、复制和打开验证页 UI，并清理专用文案与样式。
4. 更新 Rust 状态与命令测试，固化四个 Tauri 账号命令。
5. 删除 `AccountStatus::DevicePending`、设备码命令、`service/device_code.rs` 和专用测试。
6. 将仍有价值的登录完成、并发和取消测试改为浏览器登录场景。
7. 保留 `LoginStartResponse::ChatgptDeviceCode` 的解析和安全取消分支，避免外部协议异常时遗留登录尝试或泄露数据。
8. 更新架构决策、A1 验证记录和本地状态文件，明确旧设备码设计已被本决策取代。

## 验证命令

在 `apps/desktop`：

```text
pnpm test
pnpm build
pnpm format
```

在 `apps/desktop/src-tauri`：

```text
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

最后运行 `git diff --check`，检查差异规模和敏感信息，并执行一次不记录认证数据的真实浏览器登录启动/取消回归。

## 提交边界

- 只修改 Rivloom Desktop 与 `rivloom-docs`。
- 不修改 `codex-rs` 或 App Server API。
- 不包含其他 worktree、A2 文档分支或主工作区行尾噪音。
