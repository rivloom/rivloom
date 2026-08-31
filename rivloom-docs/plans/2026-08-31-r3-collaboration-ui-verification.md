# R3 协作界面验证

日期：2026-08-31。起点为最新 origin/main `8b2190bfa6996cba654fdeeb54ae237270b7d883`，
独立 worktree `C:/project/opencohive/.worktrees/r3-collaboration-ui`；原主目录旧 main 和未提交修改不动。
Node 桌面 #86–#89 已按精确 Head 普通合并，独立主干验证见 Node 验证记录。

## 类型、桥接和输入边界

14 个桌面命令的 TypeScript 参数/结果与 Rust DTO 对齐，不增加命令或 Runtime 调用。
导入桥接不会监听、连接、登记或重试；未知错误只呈现有界类别。
descriptor 输入最多 8 KiB，邀请输入最多 2 KiB；拒绝错误结构、未知字段、无效 ID/字节和过期邀请。
邀请须属于同一 Brain，剩余有效期不得超过 10 分钟。SHA-256 指纹必须匹配独立提供的 64 位小写十六进制值。
预览不是信任判定或证书验证；原始 descriptor 字符串保留给 Rust 的严格解析/TLS 校验，前端 JSON.parse
不声称能检测重复字段。邀请只允许临时传递，不进入持久化状态或诊断。
5 项新增行为测试覆盖命令接线、无隐式重试、指纹替换、Brain 绑定、输入上限、邀请期限和错误脱敏。

## 验证及保留限制

第一批：100 前端测试、TypeScript/Vite、334 + 4 + 4 + 4 Rust、cargo check、桌面 Rust 格式通过。
前端 `pnpm format --end-of-line auto` 通过；默认格式检查保留既有 75 文件换行提示，
未做整树格式化。仓库级 `just fmt` 因既有 Python 启动器故障退出 101，不计为通过。

界面组件、操作状态机和浏览器验证将在后续小 PR 中接入；此批没有可见 UI 变化。
不恢复 CI、不修改 codex-rs、不处理 #37/#38、不进入 R4/第二 Runtime/Marketplace/Skill Directory。
两台真实 Windows 设备 Gate R3、凭证过期/不完整登记恢复、桌面 capability announcement 仍待完成。
R2-FU1 elevated 多 Home 共存及真实执行/取消/边界/cleanup 继续延期；不可据自动测试宣称已通过。
