import { useCallback, useEffect, useRef, useState } from "react";

import {
  listRecentProjects,
  removeRecentProject,
  selectProject,
} from "../lib/projectBridge";
import type {
  LocalProject,
  PersistenceWarning,
  ProjectSelection,
} from "../types/project";

export type RecentProjectsState =
  | { state: "loading" }
  | { state: "empty" }
  | { state: "ready"; projects: LocalProject[] }
  | { state: "error"; message: string; projects: LocalProject[] };

export type RecentProjectAction =
  | { type: "refresh" }
  | { type: "select" }
  | { type: "remove"; projectId: string };

const loadingState: RecentProjectsState = { state: "loading" };
const errorMessage = "最近项目暂时不可用。";

function projectsOf(state: RecentProjectsState): LocalProject[] {
  return state.state === "ready" || state.state === "error"
    ? state.projects
    : [];
}

function projectsState(projects: LocalProject[]): RecentProjectsState {
  return projects.length === 0
    ? { state: "empty" }
    : { state: "ready", projects };
}

export function useRecentProjects(enabled: boolean) {
  const [state, setState] = useState<RecentProjectsState>(loadingState);
  const [pendingAction, setPendingAction] =
    useState<RecentProjectAction | null>(null);
  const [warning, setWarning] = useState<PersistenceWarning | null>(null);
  const stateRef = useRef(state);
  const enabledRef = useRef(enabled);
  const lifecycleRef = useRef(0);
  const initialPendingRef = useRef(false);
  const actionRef = useRef<symbol | null>(null);
  stateRef.current = state;
  enabledRef.current = enabled;

  useEffect(() => {
    const lifecycle = ++lifecycleRef.current;
    actionRef.current = null;
    initialPendingRef.current = enabled;
    setPendingAction(null);
    setState(loadingState);
    if (enabled) {
      void listRecentProjects()
        .then((projects) => {
          if (lifecycleRef.current === lifecycle) {
            setState(projectsState(projects));
          }
        })
        .catch(() => {
          if (lifecycleRef.current === lifecycle) {
            setState({ state: "error", message: errorMessage, projects: [] });
          }
        })
        .finally(() => {
          if (lifecycleRef.current === lifecycle) {
            initialPendingRef.current = false;
          }
        });
    }
    return () => {
      lifecycleRef.current += 1;
      actionRef.current = null;
      initialPendingRef.current = false;
    };
  }, [enabled]);

  const runAction = useCallback(
    async <T>(
      action: RecentProjectAction,
      call: () => Promise<T>,
      apply: (value: T) => void,
    ): Promise<T | null> => {
      if (
        !enabledRef.current ||
        initialPendingRef.current ||
        actionRef.current !== null
      ) {
        return null;
      }
      const token = Symbol(action.type);
      const lifecycle = lifecycleRef.current;
      actionRef.current = token;
      setPendingAction(action);
      try {
        const value = await call();
        if (enabledRef.current && lifecycleRef.current === lifecycle) {
          apply(value);
          return value;
        }
      } catch {
        if (enabledRef.current && lifecycleRef.current === lifecycle) {
          setState({
            state: "error",
            message: errorMessage,
            projects: projectsOf(stateRef.current),
          });
        }
      } finally {
        if (actionRef.current === token) {
          actionRef.current = null;
          if (lifecycleRef.current === lifecycle) setPendingAction(null);
        }
      }
      return null;
    },
    [],
  );

  const refresh = useCallback(async () => {
    await runAction({ type: "refresh" }, listRecentProjects, (projects) =>
      setState(projectsState(projects)),
    );
  }, [runAction]);

  const select = useCallback(async (): Promise<ProjectSelection | null> => {
    return runAction({ type: "select" }, selectProject, (selection) => {
      if (!selection) return;
      setWarning(selection.warning);
      setState((current) =>
        projectsState(
          [
            selection.project,
            ...projectsOf(current).filter(
              (project) => project.id !== selection.project.id,
            ),
          ].slice(0, 20),
        ),
      );
    });
  }, [runAction]);

  const remove = useCallback(
    async (projectId: string) => {
      await runAction(
        { type: "remove", projectId },
        () => removeRecentProject(projectId),
        () => {
          setWarning(null);
          setState((current) =>
            projectsState(
              projectsOf(current).filter((project) => project.id !== projectId),
            ),
          );
        },
      );
    },
    [runAction],
  );

  return { pendingAction, refresh, remove, select, state, warning };
}
