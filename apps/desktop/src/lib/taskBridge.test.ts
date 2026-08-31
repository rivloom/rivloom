import { beforeEach, describe, expect, it, vi } from "vitest";

import type { LocalTaskUpdate, TaskRecord } from "../types/task";

const tauriMocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: tauriMocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauriMocks.listen }));

import {
  listLocalTasks,
  onLocalTaskChanged,
  startLocalTask,
  stopLocalTask,
} from "./taskBridge";

const task: TaskRecord = {
  id: "task-v1-a",
  spec: { goal: "修复登录", constraints: ["保持兼容"] },
  status: "running",
  summary: null,
  error: null,
  runs: [
    {
      id: "run-v1-a",
      status: "running",
      summary: null,
      error: null,
      receipt: null,
    },
  ],
  events: [],
};

describe("taskBridge", () => {
  beforeEach(() => {
    tauriMocks.invoke.mockReset();
    tauriMocks.listen.mockReset();
  });

  it("uses only the fixed local task commands and exact correlation fields", async () => {
    tauriMocks.invoke
      .mockResolvedValueOnce([task])
      .mockResolvedValueOnce({ task, runId: "run-v1-a" })
      .mockResolvedValueOnce(task);

    await expect(listLocalTasks("project-v1-a")).resolves.toEqual([task]);
    await expect(
      startLocalTask("project-v1-a", "request-a", "修复登录", ["保持兼容"]),
    ).resolves.toEqual({ task, runId: "run-v1-a" });
    await expect(
      stopLocalTask("project-v1-a", "task-v1-a", "run-v1-a"),
    ).resolves.toEqual(task);

    expect(tauriMocks.invoke.mock.calls).toEqual([
      ["list_local_tasks", { projectId: "project-v1-a" }],
      [
        "start_local_task",
        {
          projectId: "project-v1-a",
          idempotencyKey: "request-a",
          goal: "修复登录",
          constraints: ["保持兼容"],
        },
      ],
      [
        "stop_local_task",
        {
          projectId: "project-v1-a",
          taskId: "task-v1-a",
          runId: "run-v1-a",
        },
      ],
    ]);
  });

  it("forwards bounded task events and returns Tauri's cleanup function", async () => {
    const update: LocalTaskUpdate = {
      projectId: "project-v1-a",
      task,
      patch: null,
    };
    const unlisten = vi.fn();
    const listener = vi.fn();
    tauriMocks.listen.mockImplementation(async (eventName, handler) => {
      expect(eventName).toBe("task-run-changed");
      handler({ payload: update });
      return unlisten;
    });

    await expect(onLocalTaskChanged(listener)).resolves.toBe(unlisten);
    expect(listener).toHaveBeenCalledWith(update);
  });
});
