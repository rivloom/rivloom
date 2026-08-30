# ADR-0005：采用外部 Agent Runtime

## 状态

Accepted

## 背景

Rivloom 的最终目标是多 Agent 与多人协作。若直接把 Codex 的内部 crate 作为自己的内核，
Rivloom 会承担模型循环、工具、沙箱、上下文和上游同步成本，并把产品绑定到一个 Runtime
的内部实现。若自己重新实现通用 harness，则会在抵达协作闭环前投入大量基础设施工作。

当前桌面端已经把 `codex-app-server` 作为受监管的外部进程使用，`apps/desktop` 不依赖
`codex-*` crate。这一边界已被 A0–A2 的真实实现验证。

## 决策

- Rivloom 拥有身份、Node、任务、委派、权限、Artifact、RunReceipt 和审查。
- Agent loop 由外部 Runtime 执行；Rivloom 不嵌入 `codex-core`，也不自己重写 Codex。
- 首个 Runtime 是与桌面版本配套的 `codex-app-server` sidecar。
- Tauri 继续负责进程监管、协议桥接、数据边界和事件归一化。
- 优先使用 Runtime 的结构化协议；PTY/CLI 解析只允许作为单个适配器的降级能力。
- 第一条多人协作闭环完成前，只实现具体的 `CodexRuntime`，不提前构造万能适配器。
- 第二个 Runtime 接入时，再从两个真实实现提取最小公共契约。
- 每个新增 Runtime 在实现前必须完成固定版本的许可证、再分发、服务条款和品牌审查。

## 结果

### 正面

- 现有 A0–A2 可直接复用，不需要重写桌面外壳和登录。
- Rivloom 能把工程投入集中在多人委派与审查，而不是 Agent 内核。
- Runtime 可以独立升级、崩溃和替换，故障边界更清晰。
- 不修改 `codex-core`，降低与上游同步的冲突和许可证追踪成本。

### 负面

- 需要维护 sidecar 打包、版本配套和协议兼容。
- 不同 Runtime 的认证、事件、审批和 Artifact 能力不会完全一致。
- 用户自行安装的 Runtime 可能带来发现、版本和支持差异。

### 中性

- 独立进程并不免除捆绑二进制的许可证与 NOTICE 义务。
- 完整 Chat UI 可以作为 Runtime 详情能力存在，但不是近期产品骨架。

## 考虑过的替代方案

**把 Codex 开源 crate 直接嵌入 Rivloom**

- 未采用：与内部 API 强耦合、构建和同步过重，也不能自然验证多 Runtime。

**Rivloom 自己实现通用 Agent harness**

- 未采用：需要先重做模型循环、工具、沙箱和上下文，距离多人协作目标更远。

**直接拉起任意 CLI 并解析终端文本**

- 未作为主方案：可以覆盖更多工具，但协议脆弱、审批和结构化 Artifact 难以可靠表达。

**一开始同时支持多个 Runtime**

- 未采用：会在没有真实协作闭环的情况下固化错误抽象并放大测试矩阵。

## 参考资料

- [Runtime Host 与协作闭环设计](../plans/2026-08-30-runtime-host-collaboration-design.md)
- [ADR-0001：采用 Tauri、React 与 App Server sidecar](0001-use-tauri-react-and-app-server-sidecar.md)
- Repository `LICENSE` and `NOTICE`
