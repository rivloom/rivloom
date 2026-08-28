import userEvent from "@testing-library/user-event";
import { cleanup, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const hookMocks = vi.hoisted(() => ({
  useAccountStatus: vi.fn(),
  useRecentProjects: vi.fn(),
  useRuntimeStatus: vi.fn(),
}));

vi.mock("../hooks/useAccountStatus", () => ({
  useAccountStatus: hookMocks.useAccountStatus,
}));
vi.mock("../hooks/useRecentProjects", () => ({
  useRecentProjects: hookMocks.useRecentProjects,
}));
vi.mock("../hooks/useRuntimeStatus", () => ({
  useRuntimeStatus: hookMocks.useRuntimeStatus,
}));

import { App } from "./App";

const project = {
  id: "project-a",
  path: "C:\\work\\project-a",
  name: "Project A",
  lastOpenedAt: 1_787_827_600,
  availability: "available" as const,
};

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

describe("App", () => {
  beforeEach(() => {
    hookMocks.useRuntimeStatus.mockReset();
    hookMocks.useAccountStatus.mockReset();
    hookMocks.useRecentProjects.mockReset();
    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: { state: "starting" },
    });
    hookMocks.useAccountStatus.mockReturnValue(account({ state: "signedOut" }));
    hookMocks.useRecentProjects.mockReturnValue(projects());
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

  it("gates local projects until the runtime and account are ready", () => {
    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: {
        state: "connected",
        appVersion: "0.1.0",
        appServerUserAgent: "codex-app-server/test",
        platform: "windows",
        codexHome: "C:\\rivloom-data",
      },
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
    expect(screen.getByText("本地项目与会话")).toBeInTheDocument();
  });

  it("marks the project chosen from the recent list", async () => {
    const user = userEvent.setup();
    hookMocks.useRuntimeStatus.mockReturnValue({
      retry: vi.fn(),
      retrying: false,
      status: {
        state: "connected",
        appVersion: "0.1.0",
        appServerUserAgent: "codex-app-server/test",
        platform: "windows",
        codexHome: "C:\\rivloom-data",
      },
    });
    hookMocks.useAccountStatus.mockReturnValue(
      account({ state: "signedIn", email: null, planType: "plus" }),
    );
    hookMocks.useRecentProjects.mockReturnValue({
      ...projects(),
      state: { state: "ready", projects: [project] },
    });
    render(<App />);

    await user.click(
      screen.getByRole("button", { name: "打开项目 Project A" }),
    );
    expect(screen.getByText("当前项目")).toBeInTheDocument();
  });
});
