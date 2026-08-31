import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { LocalTaskUpdate, TaskRecord } from "../types/task";

const bridgeMocks = vi.hoisted(() => ({
  listLocalTasks: vi.fn(),
  onLocalTaskChanged: vi.fn(),
  startLocalTask: vi.fn(),
  stopLocalTask: vi.fn(),
}));

vi.mock("../lib/taskBridge", () => bridgeMocks);

import { useTasks } from "./useTasks";

function task(
  id: string,
  status: TaskRecord["status"] = "running",
): TaskRecord {
  const runStatus = status === "awaitingReview" ? "completed" : status;
  return {
    id,
    spec: { goal: `目标 ${id}`, constraints: [] },
    status,
    summary: null,
    error: null,
    runs: [
      {
        id: `run-${id}`,
        status: runStatus as TaskRecord["runs"][number]["status"],
        summary: null,
        error: null,
        receipt: null,
      },
    ],
    events: [],
  };
}

describe("useTasks", () => {
  beforeEach(() => {
    bridgeMocks.listLocalTasks.mockReset();
    bridgeMocks.onLocalTaskChanged.mockReset();
    bridgeMocks.startLocalTask.mockReset();
    bridgeMocks.stopLocalTask.mockReset();
    bridgeMocks.listLocalTasks.mockResolvedValue([]);
    bridgeMocks.onLocalTaskChanged.mockResolvedValue(vi.fn());
  });

  it("loads persisted tasks and cleans up its event subscription", async () => {
    const unlisten = vi.fn();
    bridgeMocks.listLocalTasks.mockResolvedValue([task("task-a")]);
    bridgeMocks.onLocalTaskChanged.mockResolvedValue(unlisten);

    const { result, unmount } = renderHook(() => useTasks("project-a", true));

    await waitFor(() => expect(result.current.state.state).toBe("ready"));
    expect(bridgeMocks.listLocalTasks).toHaveBeenCalledWith("project-a");
    expect(bridgeMocks.onLocalTaskChanged).toHaveBeenCalledOnce();

    unmount();
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("does not hide a failed live subscription behind a successful list", async () => {
    let finishList: ((tasks: TaskRecord[]) => void) | undefined;
    bridgeMocks.listLocalTasks.mockImplementation(
      () =>
        new Promise<TaskRecord[]>((resolve) => {
          finishList = resolve;
        }),
    );
    bridgeMocks.onLocalTaskChanged.mockRejectedValue(
      new Error("raw transport detail"),
    );

    const { result } = renderHook(() => useTasks("project-a", true));
    await waitFor(() => expect(result.current.state.state).toBe("error"));

    await act(async () => finishList?.([task("task-a")]));

    expect(result.current.state).toEqual({
      state: "error",
      error: "taskUnavailable",
      tasks: [task("task-a")],
    });
  });

  it("isolates old events after a project switch and keeps patches volatile", async () => {
    const listeners: Array<(update: LocalTaskUpdate) => void> = [];
    const unlistenA = vi.fn();
    bridgeMocks.listLocalTasks.mockImplementation(async (projectId) => [
      task(`task-${projectId}`),
    ]);
    bridgeMocks.onLocalTaskChanged
      .mockImplementationOnce(async (listener) => {
        listeners.push(listener);
        return unlistenA;
      })
      .mockImplementationOnce(async (listener) => {
        listeners.push(listener);
        return vi.fn();
      });

    const { result, rerender } = renderHook(
      ({ projectId }) => useTasks(projectId, true),
      { initialProps: { projectId: "project-a" } },
    );
    await waitFor(() => expect(result.current.state.state).toBe("ready"));

    rerender({ projectId: "project-b" });
    await waitFor(() =>
      expect(bridgeMocks.listLocalTasks).toHaveBeenLastCalledWith("project-b"),
    );
    await waitFor(() => expect(listeners).toHaveLength(2));

    act(() => {
      listeners[0]({
        projectId: "project-a",
        task: task("stale", "awaitingReview"),
        patch: null,
      });
      listeners[1]({
        projectId: "project-b",
        task: task("fresh", "awaitingReview"),
        patch: {
          baselineCommit: "a".repeat(40),
          state: "complete",
          limitBytes: 524_288,
          byteCount: 12,
          sha256: "b".repeat(64),
          patch: "+changed\n",
        },
      });
    });

    expect(unlistenA).toHaveBeenCalledOnce();
    expect(result.current.tasks.map(({ id }) => id)).not.toContain("stale");
    expect(result.current.tasks.map(({ id }) => id)).toContain("fresh");
    expect(result.current.patches.fresh?.patch).toBe("+changed\n");
  });

  it("creates one normalized task with a fresh idempotency key", async () => {
    const created = task("created");
    bridgeMocks.startLocalTask.mockResolvedValue({
      task: created,
      runId: "run-created",
    });
    const { result } = renderHook(() => useTasks("project-a", true));
    await waitFor(() => expect(result.current.state.state).toBe("empty"));

    await act(async () => {
      await Promise.all([
        result.current.startTask("  修复登录  ", ["  保持兼容  ", "  "]),
        result.current.startTask("第二次", []),
      ]);
    });

    expect(bridgeMocks.startLocalTask).toHaveBeenCalledOnce();
    expect(bridgeMocks.startLocalTask).toHaveBeenCalledWith(
      "project-a",
      expect.any(String),
      "修复登录",
      ["保持兼容"],
    );
    expect(result.current.tasks).toEqual([created]);
    expect(result.current.activeRunId).toBe("run-created");
  });

  it("stops only the correlated run and never invents success on disconnect", async () => {
    const running = task("task-a");
    bridgeMocks.listLocalTasks.mockResolvedValue([running]);
    bridgeMocks.stopLocalTask.mockResolvedValue(running);
    const { result, rerender } = renderHook(
      ({ connected }) => useTasks("project-a", connected),
      { initialProps: { connected: true } },
    );
    await waitFor(() => expect(result.current.tasks).toEqual([running]));

    await act(async () => {
      await result.current.stopTask("task-a", "run-task-a");
    });
    expect(bridgeMocks.stopLocalTask).toHaveBeenCalledWith(
      "project-a",
      "task-a",
      "run-task-a",
    );

    rerender({ connected: false });
    await act(async () => {
      await result.current.startTask("不会启动", []);
    });
    expect(bridgeMocks.startLocalTask).not.toHaveBeenCalled();
    expect(result.current.tasks[0]?.status).toBe("running");
  });

  it.each(["start", "stop", "event"] as const)(
    "does not let a stale %s result overwrite a newer terminal event",
    async (source) => {
      const running = task("task-a");
      const completed = task("task-a", "awaitingReview");
      completed.events = [
        {
          sequence: 1,
          kind: {
            type: "taskStatusChanged",
            from: "running",
            to: "awaitingReview",
          },
        },
      ];
      let listener!: (update: LocalTaskUpdate) => void;
      bridgeMocks.listLocalTasks.mockResolvedValue([running]);
      bridgeMocks.onLocalTaskChanged.mockImplementation(async (callback) => {
        listener = callback;
        return vi.fn();
      });
      const complete = () =>
        listener({ projectId: "project-a", task: completed, patch: null });
      bridgeMocks.startLocalTask.mockImplementation(async () => {
        complete();
        return { task: running, runId: "run-task-a" };
      });
      bridgeMocks.stopLocalTask.mockImplementation(async () => {
        complete();
        return running;
      });
      const { result } = renderHook(() => useTasks("project-a", true));
      await waitFor(() => expect(result.current.tasks).toEqual([running]));
      await act(async () => {
        if (source === "start") await result.current.startTask("goal", []);
        else if (source === "stop")
          await result.current.stopTask("task-a", "run-task-a");
        else {
          complete();
          listener({ projectId: "project-a", task: running, patch: null });
        }
      });
      expect(result.current.tasks).toEqual([completed]);
    },
  );
});
