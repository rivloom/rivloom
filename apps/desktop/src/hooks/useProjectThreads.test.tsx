import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ProjectThread, ProjectThreadPage } from "../types/project";

const bridgeMocks = vi.hoisted(() => ({
  listProjectThreads: vi.fn(),
  readProjectThread: vi.fn(),
  startProjectThread: vi.fn(),
}));

vi.mock("../lib/projectBridge", () => bridgeMocks);

import { useProjectThreads } from "./useProjectThreads";

const threadA: ProjectThread = {
  id: "thr-a",
  name: null,
  preview: "A",
  createdAt: 1,
  updatedAt: 2,
  recencyAt: 3,
  status: "idle",
  cwd: "C:\\work\\a",
};
const threadB: ProjectThread = {
  ...threadA,
  id: "thr-b",
  preview: "B",
  cwd: "C:\\work\\b",
};
const ready = (threads: ProjectThread[], nextCursor: string | null = null) => ({
  state: "ready",
  threads,
  nextCursor,
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((finish) => (resolve = finish));
  return { promise, resolve };
}

describe("useProjectThreads", () => {
  beforeEach(() => {
    bridgeMocks.listProjectThreads.mockReset();
    bridgeMocks.readProjectThread.mockReset();
    bridgeMocks.startProjectThread.mockReset();
    bridgeMocks.listProjectThreads.mockResolvedValue({
      data: [],
      nextCursor: null,
    });
  });

  it("ignores project A after switching to B and reloads on reconnect", async () => {
    const pageA = deferred<ProjectThreadPage>();
    bridgeMocks.listProjectThreads
      .mockReturnValueOnce(pageA.promise)
      .mockResolvedValueOnce({ data: [threadB], nextCursor: null })
      .mockRejectedValueOnce(new Error("offline"));
    const { rerender, result } = renderHook(
      ({ id, connected }) => useProjectThreads(id, connected),
      { initialProps: { id: "project-a", connected: true } },
    );
    await waitFor(() =>
      expect(bridgeMocks.listProjectThreads).toHaveBeenCalledOnce(),
    );
    await expect(result.current.startThread()).resolves.toBeNull();
    expect(bridgeMocks.startProjectThread).not.toHaveBeenCalled();

    rerender({ id: "project-b", connected: true });
    await waitFor(() => expect(result.current.state).toEqual(ready([threadB])));
    await act(async () => pageA.resolve({ data: [threadA], nextCursor: null }));
    expect(result.current.state).toEqual(ready([threadB]));

    rerender({ id: "project-b", connected: false });
    expect(result.current.state).toEqual({ state: "loading" });
    rerender({ id: "project-b", connected: true });
    await waitFor(() =>
      expect(result.current.state).toEqual({
        state: "error",
        message: "会话列表暂时不可用。",
        threads: [],
        nextCursor: null,
      }),
    );
  });

  it("deduplicates pagination, appends pages, and replaces on refresh", async () => {
    const more = deferred<ProjectThreadPage>();
    const refresh = deferred<ProjectThreadPage>();
    bridgeMocks.listProjectThreads
      .mockResolvedValueOnce({ data: [threadA], nextCursor: "next" })
      .mockReturnValueOnce(more.promise)
      .mockReturnValueOnce(refresh.promise);
    bridgeMocks.startProjectThread.mockResolvedValue(threadB);
    const { result } = renderHook(() => useProjectThreads("project-a", true));
    await waitFor(() => expect(result.current.state.state).toBe("ready"));

    act(() => {
      void result.current.loadMore();
      void result.current.loadMore();
    });
    expect(bridgeMocks.listProjectThreads).toHaveBeenCalledTimes(2);
    await act(async () => more.resolve({ data: [threadB], nextCursor: null }));
    expect(result.current.state).toEqual(ready([threadA, threadB]));
    let refreshing: Promise<void> | undefined;
    act(() => {
      refreshing = result.current.refresh();
    });
    await act(async () => result.current.startThread());
    expect(result.current.state).toEqual(ready([threadB, threadA]));
    await act(async () =>
      refresh.resolve({ data: [threadA], nextCursor: null }),
    );
    await expect(refreshing).resolves.toBeUndefined();
    expect(result.current.state).toEqual(ready([threadB, threadA]));
    bridgeMocks.listProjectThreads.mockRejectedValueOnce(new Error("offline"));
    await act(async () => result.current.refresh());
    expect(result.current.state).toEqual({
      state: "error",
      message: "会话列表暂时不可用。",
      threads: [threadB, threadA],
      nextCursor: null,
    });
    expect(result.current.listAction).toBeNull();
    expect(bridgeMocks.listProjectThreads.mock.calls).toEqual([
      ["project-a", null, 0],
      ["project-a", "next", 1],
      ["project-a", null, 0],
      ["project-a", null, 0],
    ]);
  });

  it("drops a failed load-more cursor while retaining loaded threads", async () => {
    bridgeMocks.listProjectThreads
      .mockResolvedValueOnce({ data: [threadA], nextCursor: "bad-cursor" })
      .mockRejectedValueOnce(new Error("offline"));
    const { result } = renderHook(() => useProjectThreads("project-a", true));
    await waitFor(() =>
      expect(result.current.state).toEqual(ready([threadA], "bad-cursor")),
    );

    await act(async () => result.current.loadMore());
    expect(result.current.state).toEqual({
      state: "error",
      message: "会话列表暂时不可用。",
      threads: [threadA],
      nextCursor: null,
    });
    await act(async () => result.current.loadMore());
    expect(bridgeMocks.listProjectThreads).toHaveBeenCalledTimes(2);
  });

  it("prepends a started thread after a list update in the same batch", async () => {
    const refreshed = deferred<ProjectThreadPage>();
    const started = deferred<ProjectThread>();
    bridgeMocks.listProjectThreads
      .mockResolvedValueOnce({ data: [threadA], nextCursor: null })
      .mockReturnValueOnce(refreshed.promise);
    bridgeMocks.startProjectThread.mockReturnValue(started.promise);
    const { result } = renderHook(() => useProjectThreads("project-a", true));
    await waitFor(() => expect(result.current.state).toEqual(ready([threadA])));

    let refreshing!: Promise<void>;
    let starting!: Promise<ProjectThread | null>;
    act(() => {
      refreshing = result.current.refresh();
      starting = result.current.startThread();
    });
    await act(async () => {
      refreshed.resolve({ data: [threadB], nextCursor: null });
      started.resolve(threadA);
      await Promise.all([refreshing, starting]);
    });

    expect(result.current.state).toEqual(ready([threadA, threadB]));
  });

  it("reaches the 500 thread cap through bounded pages", async () => {
    const pages = Array.from({ length: 10 }, (_, pageIndex) =>
      Array.from({ length: 50 }, (_, itemIndex) => ({
        ...threadA,
        id: `thread-${pageIndex * 50 + itemIndex}`,
      })),
    );
    bridgeMocks.listProjectThreads.mockImplementation(
      (_projectId: string, _cursor: string | null, loadedCount: number) => {
        const pageIndex = loadedCount / 50;
        return Promise.resolve({
          data: pages[pageIndex],
          nextCursor: `cursor-${pageIndex + 1}`,
        });
      },
    );
    const { result } = renderHook(() => useProjectThreads("project-a", true));
    await waitFor(() =>
      expect(result.current.state).toEqual(ready(pages[0], "cursor-1")),
    );

    for (let pageIndex = 1; pageIndex < pages.length; pageIndex += 1) {
      await act(async () => result.current.loadMore());
    }
    expect(result.current.state).toEqual(ready(pages.flat()));
    expect(bridgeMocks.listProjectThreads.mock.calls).toEqual(
      Array.from({ length: 10 }, (_, pageIndex) => [
        "project-a",
        pageIndex === 0 ? null : `cursor-${pageIndex}`,
        pageIndex * 50,
      ]),
    );
    await act(async () => result.current.loadMore());
    expect(bridgeMocks.listProjectThreads).toHaveBeenCalledTimes(10);
  });

  it("serializes thread actions and invalidates results on unmount", async () => {
    const started = deferred<ProjectThread>();
    const existing = [threadB];
    bridgeMocks.listProjectThreads.mockResolvedValue({
      data: existing,
      nextCursor: "next",
    });
    bridgeMocks.startProjectThread.mockReturnValue(started.promise);
    bridgeMocks.readProjectThread.mockResolvedValue(threadB);
    const { result, unmount } = renderHook(() =>
      useProjectThreads("project-a", true),
    );
    await waitFor(() =>
      expect(result.current.state).toEqual(ready(existing, "next")),
    );

    act(() => {
      void result.current.startThread();
      void result.current.startThread();
      void result.current.readThread(threadB.id);
    });
    expect(bridgeMocks.startProjectThread).toHaveBeenCalledOnce();
    expect(bridgeMocks.readProjectThread).not.toHaveBeenCalled();
    await act(async () => started.resolve(threadA));
    const withStarted = [threadA, ...existing];
    expect(result.current.state).toEqual(ready(withStarted, "next"));

    let read: ProjectThread | null = null;
    await act(async () => {
      read = await result.current.readThread(threadB.id);
    });
    expect(read).toEqual(threadB);
    bridgeMocks.startProjectThread.mockRejectedValueOnce(new Error("offline"));
    let failedStart: ProjectThread | null = threadA;
    await act(async () => {
      failedStart = await result.current.startThread();
    });
    expect(failedStart).toBeNull();
    expect(result.current.actionError).toBe("会话暂时不可用。");
    expect(result.current.threadAction).toBeNull();
    expect(result.current.state).toEqual(ready(withStarted, "next"));
    const lateRead = deferred<ProjectThread>();
    bridgeMocks.readProjectThread.mockReturnValue(lateRead.promise);
    let pending!: Promise<ProjectThread | null>;
    act(() => {
      pending = result.current.readThread("thr-late");
    });
    unmount();
    lateRead.resolve(threadB);
    await expect(pending).resolves.toBeNull();
  });
});
