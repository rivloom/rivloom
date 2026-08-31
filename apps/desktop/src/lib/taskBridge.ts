import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { LocalTaskRun, LocalTaskUpdate, TaskRecord } from "../types/task";

const TASK_RUN_CHANGED_EVENT = "task-run-changed";

export function listLocalTasks(projectId: string): Promise<TaskRecord[]> {
  return invoke<TaskRecord[]>("list_local_tasks", { projectId });
}

export function startLocalTask(
  projectId: string,
  idempotencyKey: string,
  goal: string,
  constraints: string[],
): Promise<LocalTaskRun> {
  return invoke<LocalTaskRun>("start_local_task", {
    projectId,
    idempotencyKey,
    goal,
    constraints,
  });
}

export function stopLocalTask(
  projectId: string,
  taskId: string,
  runId: string,
): Promise<TaskRecord> {
  return invoke<TaskRecord>("stop_local_task", { projectId, taskId, runId });
}

export function onLocalTaskChanged(
  listener: (update: LocalTaskUpdate) => void,
): Promise<UnlistenFn> {
  return listen<LocalTaskUpdate>(TASK_RUN_CHANGED_EVENT, (event) => {
    listener(event.payload);
  });
}
