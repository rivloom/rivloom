import userEvent from "@testing-library/user-event";
import { render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ProjectThread } from "../../types/project";

const hookMocks = vi.hoisted(() => ({
  useProjectThreads: vi.fn(),
}));

vi.mock("../../hooks/useProjectThreads", () => ({
  useProjectThreads: hookMocks.useProjectThreads,
}));

import { ProjectWorkspace } from "./ProjectWorkspace";

const project = {
  id: "project-a",
  path: "C:\\workspaces\\a-very-long-folder-name\\rivloom-demo",
  name: "Rivloom Demo",
  lastOpenedAt: 1_787_827_600,
  availability: "available" as const,
};
const thread: ProjectThread = {
  id: "thr-a",
  name: "检查本地项目会话",
  preview: "只读取归一化元数据，不载入历史消息。",
  createdAt: 1_787_827_000,
  updatedAt: 1_787_827_300,
  recencyAt: 1_787_827_600,
  status: "idle",
  cwd: project.path,
};

function threadHook(overrides: Record<string, unknown> = {}) {
  return {
    actionError: null,
    listAction: null,
    loadMore: vi.fn(),
    readThread: vi.fn().mockResolvedValue(null),
    refresh: vi.fn(),
    startThread: vi.fn().mockResolvedValue(null),
    state: { state: "empty" },
    threadAction: null,
    ...overrides,
  };
}

function workspaceSnapshot() {
  const region = screen.getByRole("region", {
    name: "项目工作区 Rivloom Demo",
  });
  const copy = region.cloneNode(true) as HTMLElement;
  copy.querySelectorAll("time").forEach((time) => {
    time.textContent = "<time>";
  });
  return [
    copy.textContent?.replace(/\s+/g, " ").trim(),
    ...within(region)
      .queryAllByRole("button")
      .map((button) =>
        [
          button.getAttribute("aria-label") ?? button.textContent,
          (button as HTMLButtonElement).disabled ? "disabled" : "enabled",
          button.getAttribute("aria-busy") === "true" ? "busy" : null,
        ]
          .filter(Boolean)
          .join(" | "),
      ),
  ];
}

describe("ProjectWorkspace", () => {
  beforeEach(() => {
    hookMocks.useProjectThreads.mockReset();
    hookMocks.useProjectThreads.mockReturnValue(threadHook());
  });

  it("keeps the selected project visible through a runtime disconnect", async () => {
    const user = userEvent.setup();
    const onBack = vi.fn();
    render(
      <ProjectWorkspace
        project={project}
        runtimeConnected={false}
        onBack={onBack}
      />,
    );

    expect(hookMocks.useProjectThreads).toHaveBeenLastCalledWith(
      project.id,
      false,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("核心服务连接中断");
    expect(screen.getByText(project.path)).toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "项目会话" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "返回项目首页" }));
    expect(onBack).toHaveBeenCalledOnce();
    expect(workspaceSnapshot()).toMatchSnapshot("disconnected workspace");
  });

  it("forwards retry and bounded pagination without exposing cwd", async () => {
    const user = userEvent.setup();
    const threads = threadHook({
      state: {
        state: "error",
        message: "会话列表暂时不可用。",
        threads: [thread],
        nextCursor: "next-page",
      },
    });
    hookMocks.useProjectThreads.mockReturnValue(threads);
    render(
      <ProjectWorkspace project={project} runtimeConnected onBack={vi.fn()} />,
    );

    await user.click(screen.getByRole("button", { name: "重新加载会话" }));
    await user.click(screen.getByRole("button", { name: "加载更多会话" }));

    expect(threads.refresh).toHaveBeenCalledOnce();
    expect(threads.loadMore).toHaveBeenCalledOnce();
    expect(hookMocks.useProjectThreads.mock.calls).toEqual([
      [project.id, true],
    ]);
    expect(workspaceSnapshot()).toMatchSnapshot("populated workspace");
  });

  it("starts a thread only after a direct click and selects its summary", async () => {
    const user = userEvent.setup();
    const started = { ...thread, id: "thr-new", status: "notLoaded" as const };
    const threads = threadHook({
      startThread: vi.fn().mockResolvedValue(started),
    });
    hookMocks.useProjectThreads.mockReturnValue(threads);
    render(
      <ProjectWorkspace project={project} runtimeConnected onBack={vi.fn()} />,
    );

    expect(threads.startThread).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "新建会话" }));

    expect(threads.startThread).toHaveBeenCalledOnce();
    await waitFor(() =>
      expect(screen.getByText("聊天与恢复将在 A3 接入")).toBeInTheDocument(),
    );
    expect(screen.getByText(started.name!)).toBeInTheDocument();
    expect(screen.getByText(started.preview)).toBeInTheDocument();
  });

  it("reads normalized metadata and keeps failures sanitized", async () => {
    const user = userEvent.setup();
    const read = {
      ...thread,
      name: "读取后的标题",
      preview: "读取后的归一化摘要。",
    };
    const threads = threadHook({
      readThread: vi.fn().mockResolvedValue(read),
      state: { state: "ready", threads: [thread], nextCursor: null },
    });
    hookMocks.useProjectThreads.mockReturnValue(threads);
    const { rerender } = render(
      <ProjectWorkspace project={project} runtimeConnected onBack={vi.fn()} />,
    );

    await user.click(
      screen.getByRole("button", {
        name: "查看会话 检查本地项目会话，状态可继续",
      }),
    );
    expect(threads.readThread).toHaveBeenCalledWith(thread.id);
    await waitFor(() =>
      expect(screen.getByText("读取后的标题")).toBeInTheDocument(),
    );
    expect(screen.getByText("读取后的归一化摘要。")).toBeInTheDocument();

    hookMocks.useProjectThreads.mockReturnValue({
      ...threads,
      actionError: "会话暂时不可用。",
      readThread: vi.fn().mockResolvedValue(null),
    });
    rerender(
      <ProjectWorkspace project={project} runtimeConnected onBack={vi.fn()} />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("会话暂时不可用。");
    expect(
      screen.queryByText(/thread\/resume|turn\/start|project\//),
    ).not.toBeInTheDocument();
  });
});
