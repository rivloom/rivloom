# ADR-0008：接受已知限制并收口 R2

## 状态

Accepted（2026-08-31）

## 背景

R2.1–R2.5 的状态机、存储、Codex 事件路由、受管 worktree、Patch Artifact、RunReceipt、
编排、命令和桌面 UI 已实现；自动化测试、安全检查和 Windows 原生视觉 Gate 已通过。
真实已登录 Codex 的诊断也证明 Runtime 崩溃、重启和结果不明时会 fail closed，不会自动
重跑或覆盖用户 checkout。

当前 Windows Runtime 的 `on-request + auto_review` 路径仍会在审批线程崩溃；目标
`never + elevated` 又受隔离 `CODEX_HOME` 与设备级沙箱账号凭据共存问题阻塞。因此真实
success、成功 RunReceipt 和正常执行窗口内的 cancel 尚未验收。用户决定不让该兼容性问题
继续阻塞 R2 收口，后续再处理。

## 决策

- R2 作为实现里程碑接受收口，状态记为“已完成，带已知 Windows Runtime 限制”。这不把
  未执行的真实 success/cancel Gate 记为通过。
- 未完成项登记为 `R2-FU1`：以受支持方式解决多 Home elevated 沙箱共存问题，
  并补做真实 success、Patch、RunReceipt、cancel、越界拒绝和 cleanup 验收。
- `R2-FU1` 不阻塞 R3 协作协议、Brain、Node 身份、连接和对账的实现；这些工作继续使用有界
  DTO、测试 Runtime 和现有 fail-closed 语义。
- `R2-FU1` 是接受 Gate R4 的硬前置，因为 Gate R4 要求两台 Node 完成真实 Codex 任务委派；
  它也是对外宣称 Windows 本地任务闭环可用之前的发布前置。
- 当前产品代码继续保持 `on-request + auto_review`。本决策不启用未验证的
  `never + elevated`，不降低沙箱强度，也不修改系统账号、凭据或用户 Codex Home。
- R3/R4 不能为绕过 `R2-FU1` 而把测试 Runtime 的成功当作真实 Codex 验收，或扩大远端权限。

## 结果

### 正面

- R3 可以开始，不再把协作协议和 Brain 的开发绑定到一个设备级 Runtime 兼容问题。
- 已交付的 R2 工程范围与未验证的真实运行能力被明确区分。
- 后续验收有明确编号和不可跳过的 Gate，不会随着里程碑状态变化而丢失。

### 负面

- 当前 Windows 构建仍不能承诺真实 Codex Task 可以成功完成或在正常窗口内取消。
- R2 的“完成”表示实现范围接受，不表示生产可用性或真实 Runtime Gate 全通过。
- 若 `R2-FU1` 长期无受支持解法，Gate R4 和 Windows 发布仍会被阻塞。

### 中性

- CI 继续按项目当前决定暂停；本决策不把 CI 状态作为完成或失败证据。
- stacked PR 仍按既有顺序审查和合并，里程碑收口不等于这些 PR 已合并。

## 考虑过的替代方案

**继续阻塞 R2**

- 未选择：会把 R3 的协议和 Brain 工作绑定到不属于 Rivloom 业务逻辑的 Runtime 共存问题。

**把真实 success/cancel 直接记为通过**

- 拒绝：与现有证据不符，会掩盖当前 Windows 构建的用户可见限制。

**立即降级沙箱或复用主 Codex Home**

- 拒绝：会改变已经接受的安全边界，且不是收口实现里程碑所必需。

## 参考资料

- [ADR-0007：R2 托管任务临时采用无审批严格沙箱](0007-temporarily-disable-managed-run-approvals.md)
- [R1/R2 Runtime Host 验证记录](../plans/2026-08-30-runtime-host-r1-r2-verification.md)
- [Runtime Host Transition Implementation Plan](../plans/2026-08-30-runtime-host-transition-plan.md)
