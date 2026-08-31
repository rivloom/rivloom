import { useCallback, useEffect, useRef, useState } from "react";

import {
  listLocalTasks,
  onLocalTaskChanged,
  startLocalTask,
  stopLocalTask,
} from "../lib/taskBridge";
import type {
  PatchArtifact,
  TaskCommandError,
  TaskRecord,
} from "../types/task";

export type TasksState =
  | { state: "loading" }
  | { state: "empty" }
  | { state: "ready"; tasks: TaskRecord[] }
  | { state: "error"; error: TaskCommandError; tasks: TaskRecord[] };

export type TaskAction =
  | { type: "start" }
  | { type: "stop"; taskId: string; runId: string };

const knownErrors = new Set<TaskCommandError>([
  "invalidTask",
  "taskUnavailable",
  "projectUnavailable",
  "identityUnavailable",
  "runtimeUnavailable",
  "runUnavailable",
  "taskCapacityReached",
]);

function tasksOf(state: TasksState): TaskRecord[] {
  return state.state === "ready" || state.state === "error" ? state.tasks : [];
}

function taskState(tasks: TaskRecord[]): TasksState {
  return tasks.length === 0 ? { state: "empty" } : { state: "ready", tasks };
}

function upsertTask(tasks: TaskRecord[], task: TaskRecord): TaskRecord[] {
  return [task, ...tasks.filter(({ id }) => id !== task.id)];
}

function latestSequence(task: TaskRecord): number {
  return task.events.at(-1)?.sequence ?? -1;
}

function mergeInitialTasks(
  loaded: TaskRecord[],
  current: TaskRecord[],
): TaskRecord[] {
  const currentById = new Map(current.map((task) => [task.id, task]));
  const merged = loaded.map((task) => {
    const newer = currentById.get(task.id);
    currentById.delete(task.id);
    return newer && latestSequence(newer) >= latestSequence(task)
      ? newer
      : task;
  });
  return [...currentById.values(), ...merged];
}

function commandError(error: unknown): TaskCommandError {
  return typeof error === "string" && knownErrors.has(error as TaskCommandError)
    ? (error as TaskCommandError)
    : "taskUnavailable";
}

export function useTasks(projectId: string | null, runtimeConnected: boolean) {
  const [state, setState] = useState<TasksState>({ state: "loading" });
  const [patches, setPatches] = useState<Record<string, PatchArtifact>>({});
  const [action, setAction] = useState<TaskAction | null>(null);
  const [actionError, setActionError] = useState<TaskCommandError | null>(null);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const projectRef = useRef(projectId);
  const connectedRef = useRef(runtimeConnected);
  const lifecycleRef = useRef(0);
  const actionTokenRef = useRef<symbol | null>(null);
  projectRef.current = projectId;
  connectedRef.current = runtimeConnected;

  const isCurrent = (lifecycle: number, id: string) =>
    lifecycleRef.current === lifecycle && projectRef.current === id;

  useEffect(() => {
    lifecycleRef.current += 1;
    const lifecycle = lifecycleRef.current;
    const id = projectId;
    let disposed = false;
    let subscriptionFailed = false;
    let unlisten: (() => void) | undefined;
    actionTokenRef.current = null;
    setAction(null);
    setActionError(null);
    setActiveRunId(null);
    setPatches({});
    setState(id ? { state: "loading" } : { state: "empty" });
    if (!id) return;

    void onLocalTaskChanged((update) => {
      if (disposed || !isCurrent(lifecycle, id) || update.projectId !== id) {
        return;
      }
      setState((current) =>
        taskState(upsertTask(tasksOf(current), update.task)),
      );
      const patch = update.patch;
      if (patch) {
        setPatches((current) => ({
          ...current,
          [update.task.id]: patch,
        }));
      }
    })
      .then((cleanup) => {
        if (disposed || !isCurrent(lifecycle, id)) cleanup();
        else unlisten = cleanup;
      })
      .catch(() => {
        subscriptionFailed = true;
        if (isCurrent(lifecycle, id)) {
          setState((current) => ({
            state: "error",
            error: "taskUnavailable",
            tasks: tasksOf(current),
          }));
        }
      });

    void listLocalTasks(id)
      .then((tasks) => {
        if (!disposed && isCurrent(lifecycle, id)) {
          setState((current) => {
            const merged = mergeInitialTasks(tasks, tasksOf(current));
            return subscriptionFailed
              ? {
                  state: "error",
                  error: "taskUnavailable",
                  tasks: merged,
                }
              : taskState(merged);
          });
        }
      })
      .catch((error) => {
        if (!disposed && isCurrent(lifecycle, id)) {
          setState((current) => ({
            state: "error",
            error: commandError(error),
            tasks: tasksOf(current),
          }));
        }
      });

    return () => {
      disposed = true;
      lifecycleRef.current += 1;
      actionTokenRef.current = null;
      unlisten?.();
    };
  }, [projectId]);

  const startTask = useCallback(async (goal: string, constraints: string[]) => {
    const id = projectRef.current;
    const normalizedGoal = goal.trim();
    const normalizedConstraints = constraints
      .map((constraint) => constraint.trim())
      .filter(Boolean);
    if (!id || !connectedRef.current || actionTokenRef.current) return null;
    if (!normalizedGoal) {
      setActionError("invalidTask");
      return null;
    }
    const token = Symbol("start");
    const lifecycle = lifecycleRef.current;
    actionTokenRef.current = token;
    setAction({ type: "start" });
    setActionError(null);
    try {
      const result = await startLocalTask(
        id,
        crypto.randomUUID(),
        normalizedGoal,
        normalizedConstraints,
      );
      if (!isCurrent(lifecycle, id)) return null;
      setState((current) =>
        taskState(upsertTask(tasksOf(current), result.task)),
      );
      setActiveRunId(result.runId);
      return result;
    } catch (error) {
      if (isCurrent(lifecycle, id)) setActionError(commandError(error));
    } finally {
      if (actionTokenRef.current === token) {
        actionTokenRef.current = null;
        if (isCurrent(lifecycle, id)) setAction(null);
      }
    }
    return null;
  }, []);

  const stopTask = useCallback(async (taskId: string, runId: string) => {
    const id = projectRef.current;
    if (!id || actionTokenRef.current) return null;
    const token = Symbol("stop");
    const lifecycle = lifecycleRef.current;
    actionTokenRef.current = token;
    setAction({ type: "stop", taskId, runId });
    setActionError(null);
    try {
      const task = await stopLocalTask(id, taskId, runId);
      if (!isCurrent(lifecycle, id)) return null;
      setState((current) => taskState(upsertTask(tasksOf(current), task)));
      return task;
    } catch (error) {
      if (isCurrent(lifecycle, id)) setActionError(commandError(error));
    } finally {
      if (actionTokenRef.current === token) {
        actionTokenRef.current = null;
        if (isCurrent(lifecycle, id)) setAction(null);
      }
    }
    return null;
  }, []);

  return {
    action,
    actionError,
    activeRunId,
    patches,
    startTask,
    state,
    stopTask,
    tasks: tasksOf(state),
  };
}
