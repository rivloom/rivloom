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
    stageTitle: "账号接入",
    stageDescription: "建立 ChatGPT 登录、恢复和退出的安全桌面流程。",
    projectStage: {
      title: "本地项目与会话",
      description: "选择本地目录，安全恢复最近项目与项目会话。",
    },
  },
  overview: {
    eyebrow: "Rivloom Desktop",
    title: "先连接本地核心，再安全接入你的 ChatGPT 账号",
    description:
      "Rivloom 正在完成账号接入基础。当前版本不会创建会话、发送模型请求或修改项目文件。",
    privacyLabel: "凭据留在本地服务",
    privacyDescription:
      "登录凭据由独立 App Server 保存，界面只接收脱敏账号状态。",
  },
  projectOverview: {
    eyebrow: "Rivloom Local",
    title: "从一个本地项目继续工作",
    description:
      "选择已有目录或重新打开最近项目。这里只登记目录元数据，不扫描文件，也不会自动发送模型请求。",
    privacyLabel: "项目内容保持原位",
    privacyDescription:
      "Rivloom 只保存有界的路径与会话摘要；打开项目不会创建模型 turn。",
  },
  project: {
    eyebrow: "Local workspace",
    title: "本地项目",
    description: "从系统目录选择器登记项目，或继续最近打开的工作目录。",
    activeLabel: "当前项目",
    actions: {
      select: "打开本地项目",
      refresh: "重新加载",
      open: (name: string) => `打开项目 ${name}`,
      remove: (name: string) => `从最近项目移除 ${name}`,
      removeShort: "移除",
    },
    recent: {
      title: "最近项目",
      description: "最多保留 20 个已登记目录",
      count: (count: number) => `${count} / 20`,
      lastOpened: "最近打开",
    },
    loading: {
      title: "正在读取最近项目…",
      description: "Rivloom 正在读取本地保存的项目元数据。",
    },
    empty: {
      title: "还没有最近项目",
      description: "使用上方按钮选择一个现有目录开始。",
    },
    availability: {
      missing: "目录已不存在",
      unreadable: "目录无法访问",
    },
    warning: {
      recentProjectsNotSaved: "项目已打开，但最近项目未能保存。",
    },
  },
  thread: {
    eyebrow: "Project threads",
    title: "项目会话",
    description:
      "查看该项目已有的会话摘要；选择会话不会恢复聊天或发送模型请求。",
    count: (count: number) => `${count} / 500`,
    untitled: "未命名会话",
    selectedLabel: "当前会话",
    updatedAt: (timestamp: string) => `最近更新 ${timestamp}`,
    status: {
      notLoaded: "未载入",
      idle: "可继续",
      systemError: "异常",
      active: "进行中",
    },
    actions: {
      read: (title: string, status: string, selected: boolean) =>
        `查看会话 ${title}，状态${status}${selected ? "，当前会话" : ""}`,
      retry: "重新加载会话",
      loadMore: "加载更多会话",
      loadingMore: "正在加载更多…",
    },
    loading: {
      title: "正在读取项目会话…",
      description: "Rivloom 正在读取这个目录下的有界会话摘要。",
    },
    empty: {
      title: "还没有项目会话",
      description: "使用“新建会话”创建第一条空会话；不会自动发送模型请求。",
    },
  },
  account: {
    eyebrow: "账号访问",
    title: "ChatGPT 账号",
    runtimeUnavailable: {
      label: "等待核心服务",
      title: "核心服务连接后可登录",
      description: "先恢复本地核心服务，账号操作将在连接就绪后开放。",
    },
    checking: {
      label: "正在检查",
      title: "正在读取账号状态…",
      description: "Rivloom 正在通过本地 App Server 确认当前登录状态。",
    },
    signedOut: {
      label: "未登录",
      title: "连接 ChatGPT 账号",
      description:
        "通过系统浏览器登录。凭据由本地 App Server 保存，不会进入页面。",
    },
    browserPending: {
      label: "等待浏览器",
      title: "请在浏览器完成登录",
      description:
        "登录页已在系统浏览器打开；完成后 Rivloom 会自动刷新账号状态。",
      hint: "正在等待浏览器确认",
    },
    signedIn: {
      label: "已登录",
      title: "ChatGPT 账号已连接",
      description: "账号凭据由本地 App Server 管理，可随时安全退出。",
      emailLabel: "账号邮箱",
      emailUnavailable: "未提供邮箱",
      planLabel: "账号方案",
    },
    error: {
      label: "暂不可用",
      title: "账号状态未能更新",
    },
    actions: {
      browserLogin: "使用浏览器登录",
      cancel: "取消登录",
      logout: "退出账号",
      retry: "重新检查",
    },
    logoutDialog: {
      eyebrow: "账号操作",
      title: "退出 ChatGPT 账号？",
      description:
        "退出只会清除 Rivloom 独立数据目录中的当前账号凭据，不会影响浏览器中的其他会话。",
      cancel: "暂不退出",
      confirm: "确认退出",
    },
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
