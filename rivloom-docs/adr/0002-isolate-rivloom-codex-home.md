# ADR-0002：隔离 Rivloom 的 Codex 数据目录

## 状态

Accepted

## 背景

Rivloom 与官方 Codex 是独立发布、独立升级和独立支持的产品。如果两个应用同时读写
同一个 `.codex` 目录，配置格式、登录令牌、对话迁移和并发写入都可能互相影响，故障
责任也难以区分。

## 决策

Rivloom 启动 App Server 时，将 `CODEX_HOME` 设置为 Rivloom 本地应用数据目录下的
专属子目录。用户首次使用 Rivloom 时单独完成 ChatGPT/Codex 登录。

React 不读取登录令牌。令牌由 App Server 管理，Tauri 只负责提供隔离目录和展示登录
状态。

## 结果

### 正面

- Rivloom 不会污染或损坏官方 Codex 的配置和对话。
- 升级、回滚、诊断和卸载边界更清晰。
- 可以独立演进 Rivloom 设置和数据迁移。
- 更适合公开商业发行和用户支持。

### 负面

- 用户首次使用 Rivloom 时需要重新登录。
- 两个产品的对话默认不会自动互通。
- 相同资源可能在本地重复占用空间。

### 中性

- 未来可以增加用户主动的一次性导入，但不能改成长期共享写入。

## 考虑过的替代方案

**直接共用官方 `.codex`**

- 未采用：并发写入、格式迁移和登录状态相互影响的风险不可接受。

**默认隔离，但允许高级用户实时共用**

- 未采用：支持和测试成本高，仍无法消除并发写入与版本兼容风险。

## 参考资料

- [Rivloom Desktop 架构设计](../plans/2026-08-24-rivloom-desktop-architecture-design.md)
- [Codex App Server documentation](https://learn.chatgpt.com/docs/app-server)
