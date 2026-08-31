import userEvent from "@testing-library/user-event";
import { cleanup, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { zhCN } from "../content/zh-CN";

const hookMocks = vi.hoisted(() => ({
  useAccountStatus: vi.fn(),
  useIdentity: vi.fn(),
  useRecentProjects: vi.fn(),
  useRuntimeStatus: vi.fn(),
  useTasks: vi.fn(),
  useCollaboration: vi.fn(),
}));

vi.mock("../hooks/useAccountStatus", () => ({
  useAccountStatus: hookMocks.useAccountStatus,
}));
vi.mock("../hooks/useIdentity", () => ({
  useIdentity: hookMocks.useIdentity,
}));
vi.mock("../hooks/useRecentProjects", () => ({
  useRecentProjects: hookMocks.useRecentProjects,
}));
vi.mock("../hooks/useRuntimeStatus", () => ({
  useRuntimeStatus: hookMocks.useRuntimeStatus,
}));
vi.mock("../hooks/useTasks", () => ({
  useTasks: hookMocks.useTasks,
}));
vi.mock("../hooks/useCollaboration", () => ({
  useCollaboration: hookMocks.useCollaboration,
}));

import { App } from "./App";

const project = {
  id: "project-a",
  path: "C:\\work\\project-a",
  name: "Project A",
  lastOpenedAt: 1_787_827_600,
  availability: "available" as const,
};
const connectedRuntime = {
  state: "connected",
  appVersion: "0.1.0",
  appServerUserAgent: "codex-app-server/test",
  platform: "windows",
  codexHome: "C:\\rivloom-data",
} as const;

function account(status: object) {
  return {
    beginChatgptLogin: vi.fn(),
    cancelLogin: vi.fn(),
    logout: vi.fn(),
    pendingAction: null,
    refresh: vi.fn(),
    status,
  };
}

function identity(brainMembership: object | null = null) {
  return {
    pendingAction: null,
    refresh: vi.fn(),
    state: {
      state: "ready",
      identity: {
        identityId: "identity-v1-11111111111111111111111111111111",
        displayName: "本机用户",
        deviceId: "device-v1-22222222222222222222222222222222",
        brainMembership,
      },
    },
  };
}

function projects() {
  return {
    pendingAction: null,
    refresh: vi.fn(),
    remove: vi.fn(),
    select: vi.fn().mockResolvedValue(null),
    state: { state: "empty" },
    warning: null,
  };
}

function tasks() {
  return {
    action: null,
    actionError: null,
    patches: {},
    startTask: vi.fn().mockResolvedValue(null),
    state: { state: "empty" },
    stopTask: vi.fn().mockResolvedValue(null),
    tasks: [],
  };
}

describe("App", () => {
  beforeEach(() => {
    hookMocks.useRuntimeStatus.mockReset();
    hookMocks.useAccountStatus.mockReset();
    hookMocks.useIdentity.mockReset();
    hookMocks.useRecentProjects.mockReset();
    hookMocks.useTasks.mockReset();
    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: { state: "starting" },
    });
    hookMocks.useAccountStatus.mockReturnValue(account({ state: "signedOut" }));
    hookMocks.useIdentity.mockReturnValue(identity());
    hookMocks.useRecentProjects.mockReturnValue(projects());
    hookMocks.useTasks.mockReturnValue(tasks());
    hookMocks.useCollaboration.mockReturnValue({
      snapshot: null,
      pending: null,
      error: null,
      reload: vi.fn(),
    });
  });

  it("exposes product navigation and the main workspace", () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { level: 1, name: "Rivloom" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("navigation", { name: "主要导航" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("main")).toBeInTheDocument();
  });

  it("shows that the core service is starting", () => {
    render(<App />);

    expect(screen.getAllByText("正在启动").length).toBeGreaterThan(0);
    expect(screen.getByText("正在准备本地核心服务…")).toBeInTheDocument();
    expect(screen.getByText("核心服务连接后可登录")).toBeInTheDocument();
  });

  it("keeps Rivloom identity separate from Codex Runtime auth and Brain state", () => {
    const { rerender } = render(<App />);

    expect(
      screen.getByRole("heading", { name: "Rivloom 身份" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Codex Runtime" }),
    ).toBeInTheDocument();
    expect(screen.getByText("本机用户")).toBeInTheDocument();
    expect(screen.getByText("连接状态见下方协作区")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Brain 协作" }),
    ).toBeInTheDocument();
    expect(hookMocks.useCollaboration).toHaveBeenCalledWith(true);

    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: connectedRuntime,
    });
    hookMocks.useAccountStatus.mockReturnValue(
      account({ state: "signedIn", email: null, planType: "plus" }),
    );
    rerender(<App />);

    expect(screen.getByText("Codex Runtime 已登录")).toBeInTheDocument();
    expect(screen.getByText("连接状态见下方协作区")).toBeInTheDocument();

    hookMocks.useIdentity.mockReturnValue(
      identity({ brainId: "brain-1", memberId: "member-1", role: "owner" }),
    );
    rerender(<App />);
    expect(screen.getByText("已加入 Brain")).toBeInTheDocument();
    expect(screen.getByText("所有者")).toBeInTheDocument();
    expect(screen.getByText("Codex Runtime 已登录")).toBeInTheDocument();
  });

  it("gates local projects until the runtime and account are ready", () => {
    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: connectedRuntime,
    });
    render(<App />);

    expect(hookMocks.useRecentProjects).toHaveBeenLastCalledWith(false);
    expect(
      screen.queryByRole("heading", { name: "本地项目" }),
    ).not.toBeInTheDocument();

    cleanup();
    hookMocks.useAccountStatus.mockReturnValue(
      account({ state: "signedIn", email: null, planType: "plus" }),
    );
    render(<App />);

    expect(hookMocks.useRecentProjects).toHaveBeenLastCalledWith(true);
    expect(
      screen.getByRole("heading", { name: "本地项目" }),
    ).toBeInTheDocument();
    expect(screen.getByText("本地项目与任务")).toBeInTheDocument();
    Object.values(zhCN.projectOverview).forEach((copy) => {
      expect(screen.getByText(copy)).toBeInTheDocument();
    });
    expect(screen.queryByText(zhCN.overview.title)).not.toBeInTheDocument();
  });

  it("keeps projects gated if the runtime drops before account state resets", () => {
    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: { state: "stopped" },
    });
    hookMocks.useAccountStatus.mockReturnValue(
      account({ state: "signedIn", email: null, planType: "plus" }),
    );

    render(<App />);

    expect(hookMocks.useRecentProjects).toHaveBeenLastCalledWith(false);
    expect(
      screen.queryByRole("heading", { name: "本地项目" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("核心服务连接后可登录")).toBeInTheDocument();
    expect(screen.getByText(zhCN.navigation.stageTitle)).toBeInTheDocument();
  });

  it("opens a selected directory and returns with its current-project marker", async () => {
    const user = userEvent.setup();
    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: connectedRuntime,
    });
    hookMocks.useAccountStatus.mockReturnValue(
      account({ state: "signedIn", email: null, planType: "plus" }),
    );
    const projectActions = projects();
    projectActions.select
      .mockResolvedValueOnce(null)
      .mockResolvedValue({ project, warning: null });
    hookMocks.useRecentProjects.mockReturnValue({
      ...projectActions,
      state: { state: "ready", projects: [project] },
    });
    const { rerender } = render(<App />);

    await user.click(screen.getByRole("button", { name: "打开本地项目" }));
    expect(projectActions.select).toHaveBeenCalledOnce();
    expect(
      screen.queryByRole("region", { name: "项目工作区 Project A" }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "打开本地项目" }));
    expect(projectActions.select).toHaveBeenCalledTimes(2);
    expect(
      screen.getByRole("region", { name: "项目工作区 Project A" }),
    ).toBeInTheDocument();
    expect(hookMocks.useTasks.mock.calls).toEqual([[project.id, true]]);

    const overviewLink = screen.getByRole("link", { name: "概览" });
    expect(overviewLink).not.toHaveAttribute("aria-current");
    await user.click(overviewLink);
    expect(
      screen.queryByRole("region", { name: "项目工作区 Project A" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("当前项目")).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: /打开项目 Project A/ }),
    );
    await user.click(screen.getByRole("button", { name: "返回项目首页" }));
    expect(screen.getByText("当前项目")).toBeInTheDocument();
  });

  it("retains the workspace through reconnect until explicit sign-out", async () => {
    const user = userEvent.setup();
    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: connectedRuntime,
    });
    hookMocks.useAccountStatus.mockReturnValue(
      account({ state: "signedIn", email: null, planType: "plus" }),
    );
    hookMocks.useRecentProjects.mockReturnValue({
      ...projects(),
      state: { state: "ready", projects: [project] },
    });
    const { rerender } = render(<App />);

    await user.click(
      screen.getByRole("button", { name: "打开项目 Project A" }),
    );
    expect(screen.getByRole("heading", { name: "本地任务" })).toBeVisible();
    expect(hookMocks.useTasks).toHaveBeenLastCalledWith(project.id, true);

    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: { state: "stopped" },
    });
    hookMocks.useAccountStatus.mockReturnValue(account({ state: "checking" }));
    rerender(<App />);

    expect(screen.getByRole("alert")).toHaveTextContent("核心服务连接中断");
    expect(screen.getByText(project.path)).toBeInTheDocument();
    expect(hookMocks.useTasks).toHaveBeenLastCalledWith(project.id, false);

    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: connectedRuntime,
    });
    rerender(<App />);
    expect(
      screen.getByRole("region", { name: "项目工作区 Project A" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "本地任务" })).toBeVisible();
    expect(hookMocks.useTasks).toHaveBeenLastCalledWith(project.id, true);

    hookMocks.useAccountStatus.mockReturnValue(
      account({ state: "signedIn", email: null, planType: "plus" }),
    );
    rerender(<App />);
    expect(screen.getByRole("heading", { name: "本地任务" })).toBeVisible();

    hookMocks.useAccountStatus.mockReturnValue(account({ state: "signedOut" }));
    rerender(<App />);
    expect(
      screen.queryByRole("region", { name: "项目工作区 Project A" }),
    ).not.toBeInTheDocument();

    hookMocks.useAccountStatus.mockReturnValue(
      account({ state: "signedIn", email: null, planType: "plus" }),
    );
    rerender(<App />);
    expect(
      screen.queryByRole("region", { name: "项目工作区 Project A" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "本地项目" }),
    ).toBeInTheDocument();
  });

  it("forwards remove and refresh actions from the project home", async () => {
    const user = userEvent.setup();
    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: connectedRuntime,
    });
    hookMocks.useAccountStatus.mockReturnValue(
      account({ state: "signedIn", email: null, planType: "plus" }),
    );
    const projectActions = projects();
    hookMocks.useRecentProjects.mockReturnValue({
      ...projectActions,
      state: { state: "ready", projects: [project] },
    });
    const { rerender } = render(<App />);

    await user.click(
      screen.getByRole("button", { name: "从最近项目移除 Project A" }),
    );
    expect(projectActions.remove).toHaveBeenCalledWith(project.id);

    hookMocks.useRecentProjects.mockReturnValue({
      ...projectActions,
      state: {
        state: "error",
        message: "最近项目暂时不可用。",
        projects: [project],
      },
    });
    rerender(<App />);
    await user.click(screen.getByRole("button", { name: "重新加载" }));
    expect(projectActions.refresh).toHaveBeenCalledOnce();
  });
});
