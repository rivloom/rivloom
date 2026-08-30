import userEvent from "@testing-library/user-event";
import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { TaskRecord } from "../../types/task";

const hookMocks = vi.hoisted(() => ({ useTasks: vi.fn() }));

vi.mock("../../hooks/useTasks", () => ({ useTasks: hookMocks.useTasks }));

import { ProjectWorkspace } from "./ProjectWorkspace";

const project = {
  id: "project-a",
  path: "C:\\workspaces\\a-very-long-folder-name\\rivloom-demo",
  name: "Rivloom Demo",
  lastOpenedAt: 1_787_827_600,
  availability: "available" as const,
};

function task(goal = "检查本地任务闭环"): TaskRecord {
  return {
    id: "task-a",
    spec: { goal, constraints: ["不修改 codex-rs"] },
    status: "running",
    summary: null,
    error: null,
    runs: [
      {
        id: "run-a",
        status: "waitingApproval",
        summary: null,
        error: null,
        receipt: null,
      },
    ],
    events: [
      {
        sequence: 1,
        kind: { type: "runRegistered", runId: "run-a" },
      },
    ],
  };
}

function taskHook(overrides: Record<string, unknown> = {}) {
  return {
    action: null,
    actionError: null,
    activeRunId: null,
    patches: {},
    startTask: vi.fn().mockResolvedValue(null),
    state: { state: "empty" },
    stopTask: vi.fn().mockResolvedValue(null),
    tasks: [],
    ...overrides,
  };
}

function workspaceSnapshot() {
  const region = screen.getByRole("region", {
    name: "项目工作区 Rivloom Demo",
  });
  return [
    region.textContent?.replace(/\s+/g, " ").trim(),
    ...within(region)
      .queryAllByRole("button")
      .map((button) =>
        [
          button.textContent,
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
    hookMocks.useTasks.mockReset();
    hookMocks.useTasks.mockReturnValue(taskHook());
  });

  it("keeps local tasks visible while Runtime drafting is offline", async () => {
    const user = userEvent.setup();
    hookMocks.useTasks.mockReturnValue(
      taskHook({
        state: { state: "ready", tasks: [task()] },
        tasks: [task()],
      }),
    );
    render(
      <ProjectWorkspace
        project={project}
        runtimeConnected={false}
        onBack={vi.fn()}
      />,
    );

    expect(hookMocks.useTasks).toHaveBeenCalledWith(project.id, false);
    expect(
      screen.getByText("核心服务连接中断").closest("[role='alert']"),
    ).toHaveTextContent("核心服务连接中断");
    expect(screen.getByText(project.path)).toBeVisible();
    expect(screen.getByText("检查本地任务闭环")).toBeVisible();
    await user.type(screen.getByLabelText("任务目标"), "离线保留的草稿");
    expect(screen.getByLabelText("任务目标")).toHaveValue("离线保留的草稿");
    expect(screen.getByRole("button", { name: "启动本地任务" })).toBeDisabled();
    expect(
      screen.queryByRole("heading", { name: "项目会话" }),
    ).not.toBeInTheDocument();
    expect(workspaceSnapshot()).toMatchSnapshot("offline task workspace");
  });

  it("starts a task from the project instead of creating a chat thread", async () => {
    const user = userEvent.setup();
    const tasks = taskHook({
      startTask: vi.fn().mockResolvedValue({ task: task(), runId: "run-a" }),
    });
    hookMocks.useTasks.mockReturnValue(tasks);
    render(
      <ProjectWorkspace project={project} runtimeConnected onBack={vi.fn()} />,
    );

    await user.type(screen.getByLabelText("任务目标"), "实现 Task UI");
    await user.type(screen.getByLabelText("执行约束（每行一条）"), "保持边界");
    await user.click(screen.getByRole("button", { name: "启动本地任务" }));

    expect(tasks.startTask).toHaveBeenCalledWith("实现 Task UI", ["保持边界"]);
    expect(screen.queryByText("新建会话")).not.toBeInTheDocument();
  });

  it("wires the exact stop correlation and sanitized task errors", async () => {
    const user = userEvent.setup();
    const tasks = taskHook({
      actionError: "runtimeUnavailable",
      state: { state: "ready", tasks: [task()] },
      stopTask: vi.fn().mockResolvedValue(task()),
      tasks: [task()],
    });
    hookMocks.useTasks.mockReturnValue(tasks);
    render(
      <ProjectWorkspace project={project} runtimeConnected onBack={vi.fn()} />,
    );

    expect(screen.getAllByRole("alert")[0]).toHaveTextContent(
      "Codex Runtime 尚未就绪。",
    );
    await user.click(screen.getByRole("button", { name: "停止这次运行" }));
    expect(tasks.stopTask).toHaveBeenCalledWith("task-a", "run-a");
    expect(workspaceSnapshot()).toMatchSnapshot("active task workspace");
  });

  it("does not carry a task or Patch into another project", () => {
    hookMocks.useTasks.mockImplementation((projectId: string) =>
      projectId === project.id
        ? taskHook({
            state: { state: "ready", tasks: [task("项目 A 的任务")] },
            tasks: [task("项目 A 的任务")],
          })
        : taskHook(),
    );
    const { rerender } = render(
      <ProjectWorkspace project={project} runtimeConnected onBack={vi.fn()} />,
    );
    expect(screen.getByText("项目 A 的任务")).toBeVisible();

    rerender(
      <ProjectWorkspace
        project={{ ...project, id: "project-b", name: "Project B" }}
        runtimeConnected
        onBack={vi.fn()}
      />,
    );

    expect(screen.getByRole("heading", { name: "Project B" })).toBeVisible();
    expect(screen.queryByText("项目 A 的任务")).not.toBeInTheDocument();
    expect(hookMocks.useTasks).toHaveBeenLastCalledWith("project-b", true);
  });
});
