import { invoke } from "@tauri-apps/api/core";

import type {
  LocalProject,
  ProjectSelection,
  ProjectThread,
  ProjectThreadPage,
} from "../types/project";

export function listRecentProjects(): Promise<LocalProject[]> {
  return invoke<LocalProject[]>("list_recent_projects");
}

export function selectProject(): Promise<ProjectSelection | null> {
  return invoke<ProjectSelection | null>("select_project");
}

export function removeRecentProject(projectId: string): Promise<void> {
  return invoke<void>("remove_recent_project", { projectId });
}

export function listProjectThreads(
  projectId: string,
  cursor: string | null,
  loadedCount: number,
): Promise<ProjectThreadPage> {
  return invoke<ProjectThreadPage>("list_project_threads", {
    projectId,
    cursor,
    loadedCount,
  });
}

export function startProjectThread(projectId: string): Promise<ProjectThread> {
  return invoke<ProjectThread>("start_project_thread", { projectId });
}

export function readProjectThread(
  projectId: string,
  threadId: string,
): Promise<ProjectThread> {
  return invoke<ProjectThread>("read_project_thread", {
    projectId,
    threadId,
  });
}
