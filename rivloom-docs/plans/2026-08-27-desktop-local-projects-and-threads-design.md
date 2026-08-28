# Rivloom Desktop A2 本地项目与会话设计草案

- 状态：待评审
- 日期：2026-08-27
- 基准：`27f9a2d03c782ed151ef43b2f951eb64cd2e578b`
- 决策：Rivloom 本地最近项目 + 稳定 App Server `cwd` thread API

## 1. 目标与成功标准

A2 把已完成的启动和账号基础扩展为本地项目入口。用户可以选择一个现有目录，在首页
重新打开最近项目，并在该项目下列出、创建和读取持久化 thread 摘要。整个阶段不发送
用户消息、不调用 `turn/start`，因此不会启动模型推理或修改项目文件。

完成标准：

1. 目录必须由固定 Rust/Tauri 命令打开系统对话框取得，并在后端规范化、验证和登记。
2. 最近项目最多保存 20 项，按最后打开时间倒序排列，重启后恢复。
3. 不可访问或已删除目录保留在列表中并明确标记，可由用户移除。
4. 项目 thread 使用稳定 `cwd` 精确筛选，每页和前端累计数量都有硬上限。
5. 创建和读取后的 thread 必须再次核对 `cwd`，防止跨项目误关联。
6. App Server 断开、存储损坏、权限不足和协议异常都有脱敏且可恢复的状态。

## 2. 范围与非目标

### 2.1 本阶段包含

- 单目录项目选择、最近项目和移除。
- 项目首页与 thread 列表、分页、空状态和错误状态。
- `thread/start`、`thread/list`、`thread/read` 的后端适配。
- 选择当前项目和当前 thread，但不渲染完整消息历史。
- 单元、组件、协议和 fake connection 回归测试。

### 2.2 本阶段不包含

- `turn/start`、流式消息、工具、审批和 Diff；这些属于 A3/A4。
- 实验 `project/*`、`projectId`、多根项目和云同步。
- 扫描目录内容、自动发现仓库或读取项目文件。
- 移动目录后的自动迁移、项目重命名和手动排序。
- 修改 `codex-rs`、初始化实验能力或共享官方 Codex 数据目录。
- `thread/resume`、turn 历史装载和订阅生命周期；这些与 A3 的有界历史设计一起完成。

## 3. 方案比较

| 方案                          | 优点                           | 代价                                     | 结论     |
| ----------------------------- | ------------------------------ | ---------------------------------------- | -------- |
| Rivloom 最近项目 + 稳定 `cwd` | 协议稳定、边界小、可表达空项目 | 需本地保存最近列表                       | 采用     |
| App Server 实验项目实体       | 多根、排序、归属由服务端统一   | 整条连接启用实验 API，兼容风险高         | 暂不采用 |
| 从 thread 历史反推项目        | 无额外本地文件                 | 新项目无 thread 时不可见，失效目录体验差 | 不采用   |

## 4. 高层架构

```text
React ProjectHome / ProjectWorkspace
  │  受控 bridge：选择目录、最近项目、thread 操作
  ▼
Tauri commands
  ├─ 固定后端命令调用 dialog plugin 并登记选择结果
  ├─ ProjectState / ProjectService
  │    ├─ PathValidator：规范化、目录和权限检查
  │    └─ RecentProjectStore：有界 JSON、版本和恢复
  └─ AppServerState.active_connection()
       │  克隆当前连接快照，不占用账号观察者
       ▼
codex-app-server stdio JSONL
  └─ stable thread/start|list|read + cwd
```

React 只接收后端登记后的项目 ID、目录显示值、可用性和规范化 thread 摘要。后续命令
只接受项目 ID，不接受独立 cwd；它不能读取目录内容或把任意路径变成授权。Rust 是路径
和项目归属的权威边界；App Server 是 thread 持久化和运行状态的权威边界。

## 5. 项目模型与本地存储

前后端公开模型：

```text
LocalProject {
  id: string,            // 后端派生的不透明稳定 ID，供后续命令引用
  path: string,          // 后端规范化的绝对路径，仅用于显示和持久化身份
  name: string,          // 最后一个正常路径组件，仅用于显示
  lastOpenedAt: number,  // Unix 秒
  availability: "available" | "missing" | "unreadable"
}
```

文件位于 Rivloom `app_local_data_dir/settings/recent-projects-v1.json`，与
`codex-home` 并列。磁盘结构包含 `version: 1` 和最多 20 个记录。保存前去重、截断和按
时间排序；未知字段向前兼容忽略。无法解析的文件视为空列表并记录不含文件内容的诊断，
不能阻止应用启动。

新选择路径使用 `dunce::canonicalize`，避免 Windows `\\?\` 显示前缀，同时解析 `.`、
`..` 和符号链接。后端确认目标存在且为目录。不能无损转换为 App Server UTF-8 `cwd`
的路径以固定脱敏错误拒绝，绝不使用 lossy 转换。Windows 采用规范化绝对路径的
大小写不敏感身份键；Unix 使用路径字节的精确身份，macOS 另覆盖 Unicode 组合形式。
已保存但当前不存在的路径不能再次规范化，因此保留原规范化字符串并标记失效，直到
用户移除或通过系统对话框重新登记；同一路径恢复时仍重新验证目录类型和可读性。

写入采用同目录唯一临时文件、flush/sync 和平台原子替换流程；Windows 必须使用支持
覆盖现有目标且不先删除旧文件的 replace 语义，Unix 在 rename 后尽力同步父目录。
替换失败保留旧文件并清理临时文件。应用明确采用单实例写入；若单实例保证尚未落地，
存储层必须使用跨进程锁避免丢失更新。损坏 JSON 先隔离为不含内容的诊断备份，未知未来
版本只读且绝不覆盖。失败时继续返回已验证的项目，但提示“最近项目未能保存”。

## 6. 稳定 App Server 协议映射

| 用户动作 | 方法           | 关键参数                                  | 约束                 |
| -------- | -------------- | ----------------------------------------- | -------------------- |
| 打开项目 | `thread/list`  | `cwd`, `limit: 50`, `sortKey: recency_at` | 只读首屏             |
| 加载更多 | `thread/list`  | 同一 `cwd`, 上页 `cursor`                 | 游标不由前端解释     |
| 新建会话 | `thread/start` | `cwd`                                     | 仅用户明确点击后调用 |
| 读取摘要 | `thread/read`  | `threadId`, `includeTurns: false`         | 返回后核对 `cwd`     |

所有路径进入 JSON 前均来自 Rust 权威记录。`thread/list.cwd` 和 `thread/start.cwd` 始终
使用同一个规范化字符串。响应只解析白名单字段：`id`、`preview`、
`createdAt`、`updatedAt`、`recencyAt`、`status`、`cwd` 和可选 `name`；不把未知 payload
或 rollout 路径暴露给 React。

不发送 `projectId`，不调用 `project/*`，初始化仍只有稳定 client 信息。`thread/start`
可能按 App Server 当前沙箱策略读取项目级配置或指令并记录所选 `cwd` 的信任状态；该
动作只发生在用户明确选择目录并创建会话之后。Rivloom 自身不额外扫描项目内容。

A2 不调用 `thread/resume`：稳定 resume 默认返回完整 turns，可能超过现有 4 MiB JSONL
边界并建立 loaded thread 订阅。A3 将恢复、历史分页、订阅和释放作为一个有界生命周期
统一设计。

## 7. 连接与并发

当前账号服务占用 App Server 的连接和通知观察者。A2 不增加第二观察者，也不重构为
通用事件总线。`AppServerSupervisor` 提供一个只克隆当前 `AppServerConnection` 的内部
方法，`AppServerState` 将其擦除为 `Arc<dyn ConnectionControl>` 供项目命令使用。

命令获得连接快照后立即释放 supervisor 锁，再执行有超时和 pending 上限的请求。若
期间断线，现有连接会令所有等待者失败为 `Disconnected`；UI 根据 runtime 生命周期
重新加载。这样不会让项目请求阻塞重试或关闭，也不会影响账号服务的竞态保护。

前端 hook 沿用账号阶段的 revision 模式：项目切换会递增生命周期；旧目录的 list/read
结果不得覆盖新目录。每个项目只允许一个列表分页请求和一个用户动作同时进行。前端
最多累计 500 条 thread 摘要；服务端返回超过请求 limit、游标超过 4 KiB 或关键字符串
超过约定字段上限时拒绝该页并保留已加载结果。

## 8. 页面和数据流

账号已登录且 App Server 已连接时，概览页进入“本地项目”阶段：

- 主操作“打开本地项目”启动系统目录选择器。
- 最近项目卡片展示名称、路径、最后打开时间和可用性。
- 失效项目不能进入工作区，但可重新选择或从列表移除。

进入项目后显示项目标题、返回首页、thread 列表和“新建会话”。thread 行展示名称或
预览、最近更新时间和状态。点击已有 thread 只执行 read 并核对原始 cwd；A2 展示已选中
摘要和“聊天与恢复将在下一阶段接入”，不恢复 thread，也不虚构消息内容。

选择目录只更新本地最近列表并执行 `thread/list`。新建会话只执行 `thread/start`。
任何路径都不会自动执行 `turn/start`，因此没有模型费用和项目文件变更。

## 9. 故障模式

| 故障                 | 用户表现         | 处理                             |
| -------------------- | ---------------- | -------------------------------- |
| 用户取消目录选择     | 保持当前页面     | 不显示错误、不写最近列表         |
| 路径不存在或不是目录 | 无法打开         | 返回固定本地化错误，允许重选     |
| 目录不可读           | 最近项标记不可用 | 不调用 App Server                |
| JSON 缺失或损坏      | 最近列表为空     | 诊断脱敏，应用继续启动           |
| JSON 保存失败        | 项目仍可本次打开 | 提示未保存，内存状态保留         |
| App Server 断开/超时 | thread 区域错误  | 跟随 runtime 重连后重试          |
| thread `cwd` 不匹配  | 拒绝进入         | 显示项目归属异常，不暴露 payload |
| 游标无效             | 停止分页         | 保留已加载结果并允许刷新         |
| 目录在打开后被移动   | 后续操作失败     | 返回首页并标记最近项失效         |

## 10. 安全、性能与兼容性

- 不新增前端 dialog 或文件系统 capability；固定 Rust 命令只使用官方目录对话框插件。
- Rivloom 项目服务不递归扫描项目，不读取 `.git`、环境文件或用户文件内容；App Server
  创建 thread 时仍可能按既有规则读取项目级配置和指令。
- 日志不记录完整 thread 响应、目录内容或账号信息；路径错误使用分类消息。
- 最近项目硬上限 20；thread 每页上限 50、累计上限 500；游标和响应字段有固定解析边界。
- 路径使用 `PathBuf` 处理并覆盖 Windows、macOS 和 Linux；首发验证仍是 Windows。
- 不改变 App Server 初始化内容，旧配套 sidecar 若支持既有稳定 thread API 即可工作。
- 存储 schema 带版本；未来迁移失败时保留原文件，不静默覆盖未知新版本。

## 11. 测试与验收

Rust 深比较覆盖：路径规范化、非 Unicode 拒绝、Windows/Unix 身份规则、目录类型与
不可读分类、去重、20 项上限、排序、损坏/未来版本、连续保存、替换失败保旧文件、临时
文件清理、失效目录、协议白名单、响应/游标/累计分页边界、跨项目 thread 拒绝、断线和
旧连接失败。fake connection 断言请求只出现三个稳定 thread 方法，且零 `thread/resume`、
`turn/start`、`project/*`；不可读和注入路径产生零 App Server 请求。

React 覆盖：取消选择、最近项目三种可用性、项目切换竞态、空列表、有界分页、新建、
读取、runtime 断线和错误恢复。用户可见首页与工作区增加快照；在 `1180×760` 和
`960×640` 检查无溢出、键盘焦点和长 Windows 路径换行。

验收日志只记录方法名和脱敏状态，不记录完整路径以外的项目内容。真实账号和设备码不
参与 A2 验证。

## 12. 分阶段交付

1. **A2.1 最近项目存储**：contracts、版本、20 项上限和跨平台原子替换。
2. **A2.2 路径选择核心**：后端对话框、规范化、可读性和项目登记。
3. **A2.3 稳定 thread 核心**：连接快照及 list/start/read 适配。
4. **A2.4 Tauri 命令**：只接受后端项目 ID 的固定命令。
5. **A2.5 React 边界**：固定 Bridge 与 race-safe hooks。
6. **A2.6 最近项目界面**：首页、失效项目和确定性视觉验证。
7. **A2.7 thread 工作区**：有界列表、创建、读取和 A3 占位状态。

每个阶段独立测试和提交；单个 PR 目标低于 500 行复杂逻辑、总变更不超过 800 行。
设计评审通过前不开始实现。
