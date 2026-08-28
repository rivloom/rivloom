import { useCallback, useEffect, useRef, useState } from "react";

import {
  listProjectThreads,
  readProjectThread,
  startProjectThread,
} from "../lib/projectBridge";
import type { ProjectThread } from "../types/project";

type ThreadPage = { threads: ProjectThread[]; nextCursor: string | null };
export type ProjectThreadsState =
  | { state: "loading" }
  | { state: "empty" }
  | ({ state: "ready" } & ThreadPage)
  | ({ state: "error"; message: string } & ThreadPage);

export type ProjectThreadAction =
  | { type: "start" }
  | { type: "read"; threadId: string };
export type ProjectThreadListAction = "refresh" | "loadMore";

const loadingState: ProjectThreadsState = { state: "loading" };
const listError = "会话列表暂时不可用。";
const actionErrorMessage = "会话暂时不可用。";
const maxThreads = 500;

function pageOf(state: ProjectThreadsState) {
  return state.state === "ready" || state.state === "error"
    ? { threads: state.threads, nextCursor: state.nextCursor }
    : { threads: [], nextCursor: null };
}

function pageState(
  threads: ProjectThread[],
  nextCursor: string | null,
): ProjectThreadsState {
  return threads.length === 0 && nextCursor === null
    ? { state: "empty" }
    : { state: "ready", threads, nextCursor };
}

export function useProjectThreads(
  projectId: string | null,
  runtimeConnected: boolean,
) {
  const [state, setState] = useState<ProjectThreadsState>(loadingState);
  const [listAction, setListAction] = useState<ProjectThreadListAction | null>(
    null,
  );
  const [threadAction, setThreadAction] = useState<ProjectThreadAction | null>(
    null,
  );
  const [actionError, setActionError] = useState<string | null>(null);
  const stateRef = useRef(state);
  const projectRef = useRef(projectId);
  const connectedRef = useRef(runtimeConnected);
  const lifecycleRef = useRef(0);
  const listRevisionRef = useRef(0);
  const initialPendingRef = useRef(false);
  const listTokenRef = useRef<symbol | null>(null);
  const threadTokenRef = useRef<symbol | null>(null);
  stateRef.current = state;
  projectRef.current = projectId;
  connectedRef.current = runtimeConnected;

  const isCurrent = (lifecycle: number, id: string) =>
    connectedRef.current &&
    lifecycleRef.current === lifecycle &&
    projectRef.current === id;

  const runList = useCallback(
    async (
      mode: "initial" | ProjectThreadListAction,
      cursor: string | null,
      loadedCount: number,
      append: boolean,
    ) => {
      const id = projectRef.current;
      if (!id || !connectedRef.current || listTokenRef.current) return;
      const token = Symbol(mode);
      const lifecycle = lifecycleRef.current;
      const listRevision = listRevisionRef.current;
      const isCurrentList = () =>
        isCurrent(lifecycle, id) && listRevisionRef.current === listRevision;
      listTokenRef.current = token;
      if (mode === "initial") initialPendingRef.current = true;
      setListAction(mode === "initial" ? null : mode);
      try {
        const page = await listProjectThreads(id, cursor, loadedCount);
        if (!isCurrentList()) return;
        const previous = append ? pageOf(stateRef.current).threads : [];
        const threads = [...previous, ...page.data].slice(0, maxThreads);
        setState(
          pageState(
            threads,
            threads.length === maxThreads ? null : page.nextCursor,
          ),
        );
      } catch {
        if (isCurrentList()) {
          const previous =
            mode === "initial"
              ? pageOf(loadingState)
              : pageOf(stateRef.current);
          setState({ state: "error", message: listError, ...previous });
        }
      } finally {
        if (listTokenRef.current === token) {
          listTokenRef.current = null;
          if (mode === "initial") initialPendingRef.current = false;
          if (lifecycleRef.current === lifecycle) setListAction(null);
        }
      }
    },
    [],
  );

  useEffect(() => {
    lifecycleRef.current += 1;
    listTokenRef.current = null;
    threadTokenRef.current = null;
    initialPendingRef.current = false;
    setListAction(null);
    setThreadAction(null);
    setActionError(null);
    setState(loadingState);
    if (projectId && runtimeConnected) void runList("initial", null, 0, false);
    return () => {
      lifecycleRef.current += 1;
      listTokenRef.current = null;
      threadTokenRef.current = null;
      initialPendingRef.current = false;
    };
  }, [projectId, runList, runtimeConnected]);

  const refresh = useCallback(
    () => runList("refresh", null, 0, false),
    [runList],
  );
  const loadMore = useCallback(() => {
    const { threads, nextCursor } = pageOf(stateRef.current);
    if (!nextCursor || threads.length >= maxThreads) return Promise.resolve();
    return runList("loadMore", nextCursor, threads.length, true);
  }, [runList]);

  const runThread = useCallback(
    async (
      action: ProjectThreadAction,
      call: (id: string) => Promise<ProjectThread>,
      prepend: boolean,
    ): Promise<ProjectThread | null> => {
      const id = projectRef.current;
      if (
        !id ||
        !connectedRef.current ||
        initialPendingRef.current ||
        threadTokenRef.current
      ) {
        return null;
      }
      const token = Symbol(action.type);
      const lifecycle = lifecycleRef.current;
      threadTokenRef.current = token;
      setThreadAction(action);
      setActionError(null);
      try {
        const thread = await call(id);
        if (!isCurrent(lifecycle, id)) return null;
        if (prepend) {
          listRevisionRef.current += 1;
          listTokenRef.current = null;
          setListAction(null);
          const current = pageOf(stateRef.current);
          const threads = [
            thread,
            ...current.threads.filter(({ id }) => id !== thread.id),
          ].slice(0, maxThreads);
          setState(
            pageState(
              threads,
              threads.length === maxThreads ? null : current.nextCursor,
            ),
          );
        }
        return thread;
      } catch {
        if (isCurrent(lifecycle, id)) setActionError(actionErrorMessage);
      } finally {
        if (threadTokenRef.current === token) {
          threadTokenRef.current = null;
          if (lifecycleRef.current === lifecycle) setThreadAction(null);
        }
      }
      return null;
    },
    [],
  );

  const startThread = useCallback(
    () => runThread({ type: "start" }, startProjectThread, true),
    [runThread],
  );
  const readThread = useCallback(
    (threadId: string) =>
      runThread(
        { type: "read", threadId },
        (id) => readProjectThread(id, threadId),
        false,
      ),
    [runThread],
  );

  return {
    actionError,
    listAction,
    loadMore,
    readThread,
    refresh,
    startThread,
    state,
    threadAction,
  };
}
