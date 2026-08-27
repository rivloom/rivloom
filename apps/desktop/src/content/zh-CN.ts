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
        "选择浏览器或设备码登录。凭据由本地 App Server 保存，不会进入页面。",
    },
    browserPending: {
      label: "等待浏览器",
      title: "请在浏览器完成登录",
      description:
        "登录页已在系统浏览器打开；完成后 Rivloom 会自动刷新账号状态。",
      hint: "正在等待浏览器确认",
    },
    devicePending: {
      label: "设备码待验证",
      title: "在浏览器中输入设备码",
      description: "复制一次性代码，然后打开官方验证页面完成授权。",
      urlLabel: "验证地址",
      codeLabel: "一次性代码",
      copied: "设备码已复制",
      copyFailed: "复制失败，请手动选择并复制设备码。",
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
      deviceLogin: "使用设备码登录",
      switchToDevice: "改用设备码",
      switchToBrowser: "改用浏览器登录",
      cancel: "取消登录",
      copyCode: "复制代码",
      copied: "已复制",
      openVerification: "打开验证页面",
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
