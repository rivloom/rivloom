import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { LocalProject, ProjectSelection } from "../types/project";

const bridgeMocks = vi.hoisted(() => ({
  listRecentProjects: vi.fn(),
  removeRecentProject: vi.fn(),
  selectProject: vi.fn(),
}));

vi.mock("../lib/projectBridge", () => bridgeMocks);

import { useRecentProjects } from "./useRecentProjects";

const projectA: LocalProject = {
  id: "project-a",
  path: "C:\\work\\a",
  name: "a",
  lastOpenedAt: 10,
  availability: "available",
};
const projectB: LocalProject = { ...projectA, id: "project-b", name: "b" };
const ready = (projects: LocalProject[]) => ({ state: "ready", projects });

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((finish) => (resolve = finish));
  return { promise, resolve };
}

describe("useRecentProjects", () => {
  beforeEach(() => {
    bridgeMocks.listRecentProjects.mockReset();
    bridgeMocks.removeRecentProject.mockReset();
    bridgeMocks.selectProject.mockReset();
    bridgeMocks.listRecentProjects.mockResolvedValue([]);
    bridgeMocks.removeRecentProject.mockResolvedValue(undefined);
    bridgeMocks.selectProject.mockResolvedValue(null);
  });

  it("reloads after reconnect and ignores the previous lifecycle", async () => {
    const oldRead = deferred<LocalProject[]>();
    bridgeMocks.listRecentProjects
      .mockReturnValueOnce(oldRead.promise)
      .mockResolvedValueOnce([projectB]);
    const { rerender, result } = renderHook(
      ({ enabled }) => useRecentProjects(enabled),
      { initialProps: { enabled: true } },
    );
    await waitFor(() =>
      expect(bridgeMocks.listRecentProjects).toHaveBeenCalledOnce(),
    );

    rerender({ enabled: false });
    rerender({ enabled: true });
    await waitFor(() =>
      expect(result.current.state).toEqual(ready([projectB])),
    );

    await act(async () => oldRead.resolve([projectA]));
    expect(result.current.state).toEqual(ready([projectB]));

    bridgeMocks.listRecentProjects.mockRejectedValueOnce(new Error("offline"));
    rerender({ enabled: false });
    rerender({ enabled: true });
    await waitFor(() =>
      expect(result.current.state).toEqual({
        state: "error",
        message: "最近项目暂时不可用。",
        projects: [],
      }),
    );
  });

  it("blocks actions until the initial list preserves existing projects", async () => {
    const initial = deferred<LocalProject[]>();
    const pending = deferred<ProjectSelection | null>();
    bridgeMocks.listRecentProjects.mockReturnValue(initial.promise);
    bridgeMocks.selectProject.mockReturnValue(pending.promise);
    const { result } = renderHook(() => useRecentProjects(true));
    await waitFor(() =>
      expect(bridgeMocks.listRecentProjects).toHaveBeenCalledOnce(),
    );

    await expect(result.current.select()).resolves.toBeNull();
    expect(bridgeMocks.selectProject).not.toHaveBeenCalled();
    await act(async () => initial.resolve([projectA]));
    expect(result.current.state).toEqual(ready([projectA]));

    act(() => {
      void result.current.select();
      void result.current.select();
    });
    expect(bridgeMocks.selectProject).toHaveBeenCalledOnce();
    await act(async () =>
      pending.resolve({ project: projectB, warning: "recentProjectsNotSaved" }),
    );
    expect(result.current.state).toEqual(ready([projectB, projectA]));
    expect(result.current.warning).toBe("recentProjectsNotSaved");

    await act(async () => result.current.remove(projectB.id));
    expect(result.current.state).toEqual(ready([projectA]));
    expect(result.current.warning).toBeNull();

    bridgeMocks.selectProject.mockResolvedValue(null);
    await act(async () => result.current.select());
    expect(result.current.state).toEqual(ready([projectA]));
  });

  it("moves reopened projects first without duplicates and keeps 20 entries", async () => {
    const projects = Array.from({ length: 20 }, (_, index) => ({
      ...projectA,
      id: `project-${index}`,
      name: `project-${index}`,
      lastOpenedAt: index,
    }));
    const reopened = { ...projects[10], lastOpenedAt: 100 };
    const newest = { ...projectB, id: "project-new", lastOpenedAt: 101 };
    bridgeMocks.listRecentProjects.mockResolvedValue(projects);
    bridgeMocks.selectProject
      .mockResolvedValueOnce({ project: reopened, warning: null })
      .mockResolvedValueOnce({ project: newest, warning: null });
    const { result } = renderHook(() => useRecentProjects(true));
    await waitFor(() => expect(result.current.state).toEqual(ready(projects)));

    await act(async () => result.current.select());
    const reopenedProjects = [
      reopened,
      ...projects.filter((project) => project.id !== reopened.id),
    ];
    expect(result.current.state).toEqual(ready(reopenedProjects));

    await act(async () => result.current.select());
    expect(result.current.state).toEqual(
      ready([newest, ...reopenedProjects].slice(0, 20)),
    );
  });

  it("replaces on refresh and removes by opaque project ID", async () => {
    bridgeMocks.listRecentProjects
      .mockResolvedValueOnce([projectA])
      .mockResolvedValueOnce([projectB]);
    const { result, unmount } = renderHook(() => useRecentProjects(true));
    await waitFor(() => expect(result.current.state.state).toBe("ready"));

    await act(async () => result.current.refresh());
    expect(result.current.state).toEqual(ready([projectB]));
    bridgeMocks.removeRecentProject.mockRejectedValueOnce(new Error("offline"));
    await act(async () => result.current.remove(projectB.id));
    expect(result.current.state).toEqual({
      state: "error",
      message: "最近项目暂时不可用。",
      projects: [projectB],
    });
    expect(result.current.pendingAction).toBeNull();
    await act(async () => result.current.remove(projectB.id));
    expect(result.current.state).toEqual({ state: "empty" });
    expect(bridgeMocks.removeRecentProject).toHaveBeenCalledWith(projectB.id);

    const lateSelection = deferred<ProjectSelection | null>();
    bridgeMocks.selectProject.mockReturnValue(lateSelection.promise);
    let selection!: Promise<ProjectSelection | null>;
    act(() => {
      selection = result.current.select();
    });
    unmount();
    lateSelection.resolve({ project: projectA, warning: null });
    await expect(selection).resolves.toBeNull();
  });
});
