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
    stageTitle: "身份与 Runtime",
    stageDescription: "分离协作身份与模型执行凭据，建立清晰的本机边界。",
    projectStage: {
      title: "本地项目与任务",
      description: "选择本地目录，在隔离环境中执行有界任务。",
    },
  },
  overview: {
    eyebrow: "Rivloom Desktop",
    title: "身份属于 Rivloom，模型执行交给 Codex Runtime",
    description:
      "本机身份用于未来的成员、设备与协作关系；ChatGPT 登录只授权当前 Node 上的 Codex Runtime。",
    privacyLabel: "两种身份，各守边界",
    privacyDescription:
      "Rivloom 不把 Runtime 凭据当作成员身份，也不会把它发送给未来的 Brain。",
  },
  projectOverview: {
    eyebrow: "Rivloom Local",
    title: "从一个本地项目继续工作",
    description:
      "选择已有目录或重新打开最近项目。这里只登记目录元数据，不扫描文件，也不会自动发送模型请求。",
    privacyLabel: "项目内容保持原位",
    privacyDescription:
      "Rivloom 只在本机保存项目映射与有界任务状态；打开项目不会启动模型执行。",
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
  workspace: {
    label: (name: string) => `项目工作区 ${name}`,
    eyebrow: "Local task host",
    pathLabel: "项目目录",
    actions: {
      back: "返回项目首页",
    },
    disconnected: {
      title: "核心服务连接中断",
      description:
        "已保存任务仍可查看和起草；连接恢复前不能启动新的 Codex Run。",
    },
  },
  task: {
    errors: {
      invalidTask: "任务内容不符合有界输入要求。",
      taskUnavailable: "本地任务状态暂不可用。",
      projectUnavailable: "本地项目不可用或已移动。",
      identityUnavailable: "Rivloom 本机身份暂不可用。",
      runtimeUnavailable: "Codex Runtime 尚未就绪。",
      runUnavailable: "这次运行已不再可停止。",
      taskCapacityReached: "本机并行运行已达到 32 个上限。",
    },
    list: {
      eyebrow: "Bounded task history",
      title: "本地任务",
      description: "只保留有界状态、回执和 Patch 元数据。",
      count: (count: number) => `${count} 个任务`,
      loading: {
        title: "正在读取本地任务…",
        description: "Rivloom 正在加载这个项目的持久化任务状态。",
      },
      error: {
        title: "任务状态可能不是最新",
      },
      empty: {
        title: "还没有本地任务",
        description: "在上方定义目标；启动后无需进入完整 Chat 页面。",
      },
    },
    composer: {
      label: "定义本地任务",
      eyebrow: "Local task / bounded input",
      title: "把目标交给本机 Codex",
      description:
        "Rivloom 会为这次执行创建隔离 worktree，并只把下面的有界任务正文交给 Runtime。",
      boundary: (current: number, maximum: number) =>
        `${current.toLocaleString()} / ${maximum.toLocaleString()}-byte run guard`,
      goal: {
        label: "任务目标",
        placeholder: "清楚描述要完成的结果，例如：修复登录恢复并补充回归测试。",
        hint: "描述可验收的结果，不要粘贴 Token、密钥或无界日志。",
        bytes: (current: number, maximum: number) =>
          `${current.toLocaleString()} / ${maximum.toLocaleString()} bytes`,
      },
      constraints: {
        label: "执行约束（每行一条）",
        placeholder: "保持现有存储兼容\n不修改 codex-rs",
        hint: "最多 32 条；每条 1 KiB，合计 8 KiB。空行会被忽略。",
        count: (current: number, maximum: number) => `${current} / ${maximum}`,
      },
      sharing: {
        title: "只发送目标与约束",
        description: "项目绝对路径、Runtime 凭据与完整日志不会进入任务正文。",
        runtimeRequired: "连接 Codex Runtime 后即可启动",
      },
      actions: {
        start: "启动本地任务",
        starting: "正在启动…",
      },
      errors: {
        goalTooLarge: "任务目标不能超过 4 KiB。",
        tooManyConstraints: "执行约束最多 32 条。",
        constraintTooLarge: "每条执行约束不能超过 1 KiB。",
        constraintsTooLarge: "执行约束合计不能超过 8 KiB。",
        promptTooLarge: "最终任务正文超过 1,000-byte 安全上限。",
      },
    },
    run: {
      label: (goal: string) => `任务 ${goal}`,
      eyebrow: "02 / Local run",
      constraints: "执行约束",
      taskStatus: {
        draft: "草稿",
        offered: "待接受",
        accepted: "已接受",
        running: "运行中",
        awaitingReview: "等待审查",
        approved: "已接受结果",
        rejected: "已拒绝结果",
        cancelled: "已停止",
        failed: "运行失败",
        outcomeUnknown: "结果未知",
      },
      runStatus: {
        queued: "排队中",
        running: "Codex 正在执行",
        waitingApproval: "等待本机审批",
        completed: "执行已完成",
        cancelled: "执行已停止",
        failed: "执行失败",
        outcomeUnknown: "结果无法核实",
      },
      notices: {
        waitingApproval: "请在本机 Codex 完成审批；远端不能代为批准。",
        outcomeUnknown: "运行结果无法核实，Rivloom 不会自动重跑。",
      },
      timeline: {
        eyebrow: "Bounded events",
        title: "最近状态",
        registered: "Run 已登记",
        taskChanged: (from: string, to: string) => `任务：${from} → ${to}`,
        runChanged: (from: string, to: string) => `运行：${from} → ${to}`,
      },
      receipt: {
        eyebrow: "Verifiable output",
        title: "运行回执",
        summary: "结果摘要",
        summaryUnavailable: "Runtime 没有提供结果摘要。",
        runtime: (runtime: string, version: string) =>
          `${runtime} / ${version}`,
        pending: "RunReceipt 会在运行进入可核实终态后出现。",
      },
      tests: {
        title: "测试报告",
        notReported: "测试未报告",
        exitCode: (code: number) => `退出码 ${code}`,
        passed: "通过",
        failed: "失败",
      },
      patch: {
        title: "Patch",
        bytes: (value: string) => `${value} bytes`,
        sizeUnavailable: "大小未知",
        open: "查看 Patch",
        state: {
          empty: "工作区没有可报告的改动。",
          complete: "Patch 已通过大小与哈希校验。",
          tooLarge: "Patch 超过本地展示上限，仅保留元数据。",
          unsupportedEncoding: "Patch 不是可安全展示的 UTF-8 文本。",
        },
      },
      stop: {
        action: "停止这次运行",
        stopping: "正在停止…",
      },
    },
  },
  identity: {
    eyebrow: "01 / Local identity",
    title: "Rivloom 身份",
    badge: {
      loading: "正在读取",
      error: "暂不可用",
      local: "仅此设备",
      brain: "Brain 成员",
    },
    loading: {
      title: "正在读取本机身份…",
    },
    local: {
      description:
        "这是独立于 Runtime 登录的本机协作身份；加入 Brain 前只在此设备生效。",
      unjoined: "尚未加入 Brain",
    },
    brain: {
      description: "此身份已建立 Brain 成员关系，可用于后续委派与审查。",
      joined: "已加入 Brain",
    },
    fields: {
      brain: "Brain 状态",
      role: "成员角色",
      deviceId: "设备 ID",
      identityId: "身份 ID",
    },
    roles: {
      owner: "所有者",
      member: "成员",
    },
    actions: {
      retry: "重新读取身份",
    },
  },
  runtimeSection: {
    eyebrow: "02 / Runtime host",
    title: "Codex Runtime",
    description:
      "负责本机模型执行与 ChatGPT 认证；登录状态不会改变 Rivloom 身份或 Brain 权限。",
  },
  account: {
    eyebrow: "Codex Runtime Auth",
    title: "ChatGPT 登录",
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
      title: "为 Codex Runtime 登录 ChatGPT",
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
      title: "Codex Runtime 已登录",
      description:
        "凭据只供本机 Runtime 调用模型，不代表 Rivloom 成员或 Brain 权限。",
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
