# Rivloom Desktop A2 本地项目与会话最终验证记录

- 验证日期：2026-08-28
- 验证分支：`codex/a2-final-verification`
- 已验证 `main` 提交：`f85e075b8a0ac38694d458ba3015e6d1bbaf0c9a`
- 已验证内容 tree：`471ebf7292bfb5dc52000e6733930833a7132f55`
- 状态：自动化、协议边界、静态差异和确定性视觉验证全部通过

## 结果

A2 已完成安全的本地目录选择、有界最近项目持久化，以及按稳定 `cwd` 列出、创建和读取
thread 摘要的完整桌面闭环。实现没有修改 `codex-rs`，没有启用实验 App Server API，
也没有增加前端文件系统、目录对话框或 shell 权限。

打开项目和浏览 thread 不会调用 `turn/start` 或启动模型推理。用户明确点击“新建会话”
时只调用稳定的 `thread/start`；A2 不调用 `thread/resume`，不装载 turns，也不发送用户
消息。聊天、历史恢复和流式运行仍属于 A3。

## 合并记录

| 阶段                        | PR  | `main` 合并提交                            |
| --------------------------- | --- | ------------------------------------------ |
| A2.1 最近项目存储           | #23 | `ef01b6ae686788ce603ee60f9430361a18d2a91d` |
| A2.2 路径选择与项目登记     | #24 | `a413d10b8f5f8deabf766a32d2fc3e4b557476da` |
| A2.3 有界 `cwd` thread 服务 | #25 | `36af3d945b989b41f064a4d0091a8055ebaf007d` |
| A2.4a 项目运行时注册        | #26 | `74b15082ad16bb4d6bb50607ad2192c261481300` |
| A2.4b 固定 Tauri 项目命令   | #27 | `4f1da43185923939f41bc31c7ddb044d8db1e74e` |
| A2.5a 前端 bridge contracts | #28 | `67775f0c4d21f0125e4c8b3dd9c5979ee5658925` |
| A2.5b 最近项目 hook         | #29 | `b82ef214f007e9710dc69d1e3f6b27a9182caf29` |
| A2.5c 项目 thread hook      | #30 | `11e0b679f31d88f7b5d343a36ef25c03f0f33df8` |
| A2.6a 最近项目卡片          | #31 | `590dfdbd06cb4096c184dded59078ae5df08b99f` |
| A2.6b 最近项目首页集成      | #32 | `dc7101fec3553d98fee8a54d1f9c96b73762a4ca` |
| A2.7a 有界 thread 列表      | #33 | `98855e49178e7ba2fdacc7633743ca6b10c077c6` |
| A2.7b 项目工作区            | #34 | `a5f729a33bb51c52acddc28903487c069f4b4cf5` |
| A2.7c 工作区导航闭环        | #35 | `f85e075b8a0ac38694d458ba3015e6d1bbaf0c9a` |

## 自动化验证

命令从 `apps/desktop` 运行。

| 检查                                                           | 结果                                                                                                           |
| -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `just fmt`                                                     | 通过；只产生已知的 Windows LF/CRLF 状态噪音，`git diff --stat`、`--raw`、`--numstat` 和 `--check` 均无内容差异 |
| `just check`                                                   | 通过：14 个前端测试文件、68 项测试、TypeScript 检查和 Vite 生产构建                                            |
| `just test-rust`                                               | 通过：137 项库测试和 4 项 Tauri command 测试                                                                   |
| `just check-rust`                                              | 通过                                                                                                           |
| `methods_send_only_the_three_exact_stable_requests`            | 通过                                                                                                           |
| `tauri_commands_resolve_managed_states_and_forward_parameters` | 通过                                                                                                           |

最终验证使用锁定的 pnpm `10.34.5`。临时 Corepack shim、准备的 sidecar、`node_modules`、
`target` 和 `dist` 均为未跟踪或忽略的本地前置物，不属于提交。

## 协议与安全边界

生产代码中的项目 thread 请求方法只有：

- `thread/list`
- `thread/start`
- `thread/read`

精确协议测试覆盖稳定参数、分页游标、响应字段白名单、`cwd` 归属复核、断开和有界错误。
全 A2 静态审计确认：

- `thread/resume`、`turn/start` 和 `project/*` 只出现在负向测试断言中。
- 初始化没有 `experimentalApi`，请求没有 `projectId`。
- React 只能调用固定项目命令，不能把任意路径或 `cwd` 当作授权输入。
- 目录只由 Rust 后端系统对话框选择、规范化、验证和登记。
- capability 没有新增前端 dialog、filesystem、shell、spawn 或 execute 权限。
- 没有账号方法、OAuth 临时值、模型调用或 Rivloom 项目服务文件内容读取进入 A2 流程。
- App Server 在用户明确创建 thread 时仍可能按自身既有规则读取项目级配置或指令；
  Rivloom 项目服务本身不扫描项目文件。

## 视觉验证

A2.7c 功能提交 `2fc76fc375` 与最终 squash 合并提交
`f85e075b8a0ac38694d458ba3015e6d1bbaf0c9a` 的 Git tree 都是
`471ebf7292bfb5dc52000e6733930833a7132f55`，因此提交前完成的确定性视觉证据与最终
`main` 逐文件一致。

在 `1180×760` 和 `960×640` 两种视口下检查：

- 首页空最近项目状态。
- 包含长 Windows 路径的最近项目列表。
- 缺失或不可读项目状态。
- 项目工作区空 thread 状态。
- 有数据、分页和选中摘要状态。
- App Server 断线和可恢复错误状态。

两种尺寸均无文档级横向溢出；长 Windows 路径、thread 名称和预览正确换行，主要操作、
错误语义和键盘焦点可见。视觉截图是临时验证材料，未提交到 Git。

## 最终差异审计

从设计合并提交 `ff3a348ebf0e6a7787f951beae44a82a6e3fa887` 到 A2 最终实现提交
共 49 个变更文件。审计结果：

- `codex-rs`：0 个变更文件。
- App Server schema 和实验能力：0 个变更文件。
- capability 文件净新增权限：0。
- 二进制、凭据、OAuth 临时值、`target`、`dist`：0 个提交文件。
- 生产 App Server 方法面严格限制为三个稳定 thread 方法。

## 已知限制与下一步

- A2 只显示 thread 摘要，不恢复或渲染 turns，不支持消息发送和流式事件。
- A2 不支持多根项目、目录移动迁移或实验 `project/*` 实体。
- 确定性视觉和原生平台验证在 Windows 完成；路径和持久化逻辑仍由自动化测试覆盖跨平台
  分支，后续发行阶段需在 macOS 和 Linux 重复原生验证。
- 临时截图不是仓库中的持久验证资产。

A2 完成后的唯一优先级是 A3：先设计有界 thread 恢复、历史装载和订阅生命周期，再接入
`turn/start`、消息输入和流式运行。
