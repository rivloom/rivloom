export const zhCN = {
  product: {
    name: "Rivloom",
    edition: "Desktop",
    workspace: "本地工作区",
  },
  navigation: {
    label: "主要导航",
    overview: "概览",
    stageLabel: "当前阶段",
    stageTitle: "桌面基础环境",
    stageDescription: "正在建立安全、独立的本地运行基础。",
  },
  overview: {
    eyebrow: "Rivloom Desktop",
    title: "让本地 AI 协作有一个清晰的起点",
    description:
      "Rivloom 正在准备独立的核心服务。当前版本只建立桌面运行环境，不会登录账户、调用模型或修改项目文件。",
    privacyLabel: "本地优先",
    privacyDescription: "核心服务通过本机进程通信，不开放网络监听端口。",
  },
  service: {
    eyebrow: "运行状态",
    title: "核心服务",
    starting: {
      label: "正在启动",
      title: "正在准备本地核心服务…",
      description:
        "首次启动可能需要一点时间，Rivloom 会在连接完成后显示运行信息。",
    },
    connected: {
      label: "已连接",
      title: "本地核心服务已就绪",
      description: "Rivloom 已与配套的 App Server 建立安全的本地连接。",
    },
    error: {
      label: "连接失败",
      title: "核心服务未能启动",
      description: "请检查下方提示；如果问题可以恢复，你可以重新尝试连接。",
      retry: "重试连接",
    },
    stopped: {
      label: "已停止",
      title: "核心服务已停止",
      description: "Rivloom 当前没有运行本地核心服务。",
    },
    fields: {
      appVersion: "Rivloom 版本",
      appServer: "App Server",
      platform: "运行平台",
      codexHome: "数据目录",
    },
  },
  statusBar: {
    label: "核心服务状态",
    starting: "App Server 正在启动",
    connected: "App Server 已连接",
    error: "App Server 连接失败",
    stopped: "App Server 已停止",
  },
} as const;
