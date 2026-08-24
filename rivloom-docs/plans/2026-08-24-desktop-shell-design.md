# Rivloom Desktop 空壳设计

- 状态：已确认（阶段一）
- 日期：2026-08-24
- 目标分支：`feat/desktop-shell`
- 首发平台：Windows 10/11 x64

## 1. 目标

建立 Rivloom 的第一个可运行桌面版本。它应显示清新、专业的基础界面，并由 Tauri
后端启动、初始化和关闭随应用配套的 Codex App Server。这个阶段不登录、不调用模型、
不修改项目文件，也不实现网络代理。

## 2. 用户可见范围

第一阶段只包含：

- Windows 原生标题栏和 Rivloom 窗口标题。
- 应用内部顶部栏、侧边栏、主内容区和底部状态栏。
- App Server 的启动中、已连接、失败和重试状态。
- Rivloom 版本、App Server user agent 和运行平台信息。
- 清晰的加载、空白和错误反馈。

第一阶段不包含：

- ChatGPT/Codex 登录。
- 项目选择和最近项目。
- 对话、流式输出、工具、审批和 Diff。
- 网络代理和多人协作。
- 自动更新、安装包签名和正式品牌图标。

## 3. 视觉方向

关键词是 **清新、专业、安静、可信**。默认使用浅色主题，代码结构预留深色主题，
但第一阶段不提供主题切换入口。

避免以下风格：

- 高饱和霓虹和发光效果。
- 大面积渐变。
- 厚重或多层阴影。
- 过度圆润、玩具化的组件。
- 为装饰而持续播放的动画。

## 4. 设计变量

颜色以语义命名，不在组件中散落十六进制值。

| 用途 | 变量 | 初始值 |
|---|---|---|
| 应用背景 | `--color-bg` | `#F5F9F8` |
| 面板背景 | `--color-surface` | `#FFFFFF` |
| 次级面板 | `--color-surface-muted` | `#EAF3F1` |
| 主文字 | `--color-text` | `#172421` |
| 次要文字 | `--color-text-muted` | `#5B6F69` |
| 品牌强调 | `--color-accent` | `#147D6A` |
| 强调悬停 | `--color-accent-hover` | `#0F6657` |
| 普通边框 | `--color-border` | `#D7E3E0` |
| 焦点环 | `--color-focus` | `#2DA891` |
| 错误 | `--color-danger` | `#B42318` |

已验证的关键对比度：

- 主文字在应用背景上：15.10:1。
- 次要文字在白色面板上：5.35:1。
- 白色按钮文字在强调色上：5.03:1。
- 白色按钮文字在错误色上：6.57:1。

基础尺寸：

```text
spacing: 4 / 8 / 12 / 16 / 24 / 32 px
radius: 6 / 8 / 12 px
border: 1 px
focus ring: 2 px
motion: 120 / 180 ms
```

尊重 `prefers-reduced-motion`，减少动态效果时关闭非必要位移和过渡。

## 5. 字体和商业发行策略

第一阶段只引用用户 Windows 已安装的字体，不把微软字体文件放进安装包。

界面字体栈：

```css
system-ui, "Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", sans-serif
```

代码字体栈：

```css
"Cascadia Code", Consolas, monospace
```

如果 Cascadia Code 未安装，自动回退到 Consolas。未来若捆绑任何开源字体，必须先
核对许可证，并将版权与许可证加入第三方依赖清单。

## 6. 页面骨架

```text
┌──────────────────────────────────────────────────┐
│ Rivloom                               本地工作区  │  48 px
├─────────────┬────────────────────────────────────┤
│             │                                    │
│ 侧边导航    │         主内容区域                 │
│ 240 px      │                                    │
│             │                                    │
├─────────────┴────────────────────────────────────┤
│ 核心服务状态 · App Server 版本 · 诊断入口        │  30 px
└──────────────────────────────────────────────────┘
```

- 默认窗口：1180 × 760。
- 最小窗口：960 × 640。
- 第一阶段使用 Windows 原生标题栏，不实现自定义拖动区和窗口按钮。
- 侧边栏只显示产品名、占位导航和阶段说明，不伪装尚未实现的功能。
- 主内容区显示欢迎信息和核心服务状态卡片。
- 状态栏始终显示 App Server 的真实连接状态。

## 7. 基础组件

### AppShell

拥有顶部栏、侧边栏、主内容区和底部状态栏。使用 CSS Grid，避免布局状态散落在页面
组件中。

### ServiceStatusCard

状态包括：

- `starting`：正在启动核心服务，显示短加载反馈。
- `connected`：显示连接成功、user agent、平台和数据目录。
- `error`：显示可理解的错误摘要和重试按钮。
- `stopped`：应用正在退出或服务尚未启动。

颜色不是唯一状态表达方式；每个状态同时包含文本和图形标识。

### Button

第一阶段提供主要、次要和危险三种语义。所有按钮支持键盘操作、禁用状态、加载状态和
可见焦点环。

### StatusBadge

用于状态栏和状态卡片。状态文本必须能独立说明含义，不能只显示彩色圆点。

## 8. 文案策略

第一阶段默认简体中文。用户可见文案集中在 `src/content/zh-CN.ts`，不散落在 JSX 中。
此阶段不引入完整国际化框架；以后增加英文时，可将同一结构扩展为语言资源和选择器。

## 9. 技术结构

```text
apps/desktop/
├─ src/                         React / TypeScript
│  ├─ app/                      页面装配
│  ├─ components/               基础组件
│  ├─ content/                  集中文案
│  ├─ lib/                      Tauri bridge
│  ├─ styles/                   tokens 和全局样式
│  └─ types/                    前后端共享的前端类型
├─ scripts/                     sidecar 准备脚本
└─ src-tauri/
   ├─ capabilities/             最小权限配置
   ├─ binaries/                 构建生成、不提交的 App Server
   └─ src/
      ├─ app_server/            协议与进程监管
      ├─ runtime_status.rs      对前端暴露的状态
      ├─ lib.rs                 Tauri 应用装配
      └─ main.rs                桌面入口
```

React 只能调用专门定义的 Tauri 命令，例如读取状态和请求重试。React 不获得 shell
权限，也不接收 App Server 子进程句柄。

## 10. App Server 生命周期

1. Tauri 解析 Rivloom 本地应用数据目录。
2. 在该目录下创建独立的 `codex-home`。
3. 使用 Tauri sidecar 启动配套的 `codex-app-server`，并设置 `CODEX_HOME`。
4. 发送一次 `initialize` 请求，client name 为 `rivloom_desktop`。
5. 收到成功响应后发送 `initialized` 通知。
6. 将 user agent、codex home、平台和连接状态转成受控的前端 DTO。
7. 应用退出时关闭 stdin 并终止仍存活的子进程。

所有通信使用 stdio JSONL，不开放网络监听端口。

## 11. 错误处理

- sidecar 文件缺失：显示“核心服务文件缺失”，提供重新准备或重新安装提示。
- 进程启动失败：显示系统错误摘要，详细路径只进入脱敏诊断日志。
- 初始化超时：结束子进程并进入错误状态。
- JSON 无法解析：记录协议错误，不把原始敏感内容直接显示给用户。
- 协议返回错误：保留错误 code 和安全摘要。
- 意外退出：第一阶段提供手动重试；自动重启在稳定性阶段加入。

## 12. 可访问性

- 正文和控件达到 WCAG 2.1 AA 的常规文本对比度要求。
- Tab 顺序与视觉顺序一致。
- 所有交互使用原生语义元素。
- `:focus-visible` 提供 2px 清晰焦点环。
- 动态状态通过 `aria-live="polite"` 宣布。
- 错误区域使用 `role="alert"`。
- 不依赖颜色独立表达连接状态。

## 13. 验收标准

1. `pnpm` 可以识别 `@rivloom/desktop` workspace 包。
2. React 单元测试、TypeScript 检查和生产构建通过。
3. Tauri Rust 单元测试和检查通过。
4. App Server 初始化握手成功，不调用模型。
5. UI 能显示启动、连接成功和错误三种状态。
6. 关闭窗口后不留下 App Server 进程。
7. `CODEX_HOME` 指向 Rivloom 独立目录。
8. Windows 原生窗口在 960 × 640 时无布局溢出。
9. 键盘可以访问所有第一阶段操作。
10. 官方 Codex 文件除必要 workspace 配置外不被修改。

## 14. 参考资料

- [Rivloom Desktop 总体架构](2026-08-24-rivloom-desktop-architecture-design.md)
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri frontend configuration](https://v2.tauri.app/start/frontend/)
- [Tauri external binaries](https://v2.tauri.app/develop/sidecar/)
- [Tauri security](https://v2.tauri.app/security/)
- [Microsoft font redistribution FAQ](https://learn.microsoft.com/en-us/typography/fonts/font-faq)
- [Cascadia Code license](https://github.com/microsoft/cascadia-code/blob/main/LICENSE)
