# ADR-0006：分离 Rivloom Identity 与 Runtime Auth

## 状态

Accepted

## 背景

现有 Rivloom Desktop 要求用户通过 App Server 完成 ChatGPT 登录。该认证足以让本机
Codex Runtime 调用模型，但不能表达 Rivloom 的成员、设备、Brain、角色、邀请、委派或
审查身份。未来其他 Runtime 还可能使用 API Key、浏览器 OAuth、本地配置或根本不需要
账号。

如果把 ChatGPT 账号当作 Rivloom 用户身份，产品会把协作权限错误地绑定到第三方 Runtime，
也可能诱使 Brain 收集本应只留在 Node 的凭证。

## 决策

- `Rivloom Identity` 与 `Runtime Auth` 是两个独立领域。
- Rivloom Identity 用于用户显示名、设备密钥、Brain 成员关系、角色、委派和审查。
- Runtime Auth 只回答某个 Runtime 在当前 Node 上是否可执行。
- 现有 ChatGPT 浏览器登录归属 `Codex Runtime Auth`；它不是 Rivloom 登录。
- 每个 Node 独立完成并保存自己的 Runtime 认证，继续使用隔离的 `CODEX_HOME`。
- Brain 不接收、代理或保存 Runtime OAuth Token、API Key 或认证文件。
- UI 必须分别展示“Rivloom 身份/Brain 状态”和“Codex Runtime 登录状态”。
- 第一版 Rivloom Identity 可以本地生成，不以建设中心化云账号为前置。

## 结果

### 正面

- 多人权限不依赖 ChatGPT 或任一 Runtime 的账号模型。
- 每台 Node 的凭证和费用归属保持本地且清晰。
- 未来接入不同认证方式的 Runtime 时不需要重做团队身份。
- Brain 的敏感数据面显著缩小。

### 负面

- 用户会看到两个不同状态，需要清晰文案避免“为什么登录两次”的困惑。
- 邀请、设备密钥丢失和成员撤销需要 Rivloom 自己处理。
- 一个人在多台 Node 上可能需要分别完成 Runtime 登录。

### 中性

- 未来可以增加 Rivloom 云账号，但它仍不能替代 Node 的 Runtime Auth。
- 可选的凭证同步必须另立安全与合规决策，不能由本 ADR 默认授权。

## 考虑过的替代方案

**直接使用 ChatGPT 账号作为 Rivloom 用户**

- 未采用：无法安全表达设备与团队权限，并把产品身份绑定到单一 Runtime。

**由 Brain 统一保存并下发 Runtime Token**

- 未采用：扩大泄露半径、费用和撤销边界不清，也不适用于多种认证方式。

**首版完全不建立 Rivloom Identity**

- 未采用：两 Node 之间无法可靠认证委派者和执行者，只能做演示级远程调用。

## 参考资料

- [Runtime Host 与协作闭环设计](../plans/2026-08-30-runtime-host-collaboration-design.md)
- [ADR-0002：隔离 Rivloom 的 Codex 数据目录](0002-isolate-rivloom-codex-home.md)
