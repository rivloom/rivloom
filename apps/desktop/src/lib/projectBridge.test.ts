import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  LocalProject,
  ProjectSelection,
  ProjectThread,
  ProjectThreadPage,
} from "../types/project";

const tauriMocks = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: tauriMocks.invoke }));

import * as projectBridge from "./projectBridge";

const project: LocalProject = {
  id: "project-v1-opaque",
  path: "C:\\work\\rivloom",
  name: "rivloom",
  lastOpenedAt: 42,
  availability: "available",
};
const thread: ProjectThread = {
  id: "thr-1",
  name: null,
  preview: "Preview",
  createdAt: 10,
  updatedAt: 20,
  recencyAt: 30,
  status: "idle",
  cwd: project.path,
};
const page: ProjectThreadPage = { data: [thread], nextCursor: "next" };
const selection: ProjectSelection = { project, warning: null };

describe("projectBridge", () => {
  beforeEach(() => tauriMocks.invoke.mockReset());

  it("exposes only six fixed commands with opaque IDs and camelCase params", async () => {
    tauriMocks.invoke
      .mockResolvedValueOnce([project])
      .mockResolvedValueOnce(selection)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(page)
      .mockResolvedValueOnce(thread)
      .mockResolvedValueOnce(thread);

    await expect(projectBridge.listRecentProjects()).resolves.toEqual([
      project,
    ]);
    await expect(projectBridge.selectProject()).resolves.toEqual(selection);
    await expect(
      projectBridge.removeRecentProject(project.id),
    ).resolves.toBeUndefined();
    await expect(
      projectBridge.listProjectThreads(project.id, "cursor-1", 7),
    ).resolves.toEqual(page);
    await expect(projectBridge.startProjectThread(project.id)).resolves.toEqual(
      thread,
    );
    await expect(
      projectBridge.readProjectThread(project.id, thread.id),
    ).resolves.toEqual(thread);

    expect(Object.keys(projectBridge)).toHaveLength(6);
    expect(tauriMocks.invoke.mock.calls).toEqual([
      ["list_recent_projects"],
      ["select_project"],
      ["remove_recent_project", { projectId: project.id }],
      [
        "list_project_threads",
        { projectId: project.id, cursor: "cursor-1", loadedCount: 7 },
      ],
      ["start_project_thread", { projectId: project.id }],
      ["read_project_thread", { projectId: project.id, threadId: thread.id }],
    ]);
    expect(JSON.stringify(tauriMocks.invoke.mock.calls)).not.toContain("cwd");
    expect(JSON.stringify(tauriMocks.invoke.mock.calls)).not.toContain(
      project.path,
    );
  });

  it("preserves directory dialog cancellation without inventing a project", async () => {
    tauriMocks.invoke.mockResolvedValue(null);

    await expect(projectBridge.selectProject()).resolves.toBeNull();
    expect(tauriMocks.invoke).toHaveBeenCalledWith("select_project");
  });
});
