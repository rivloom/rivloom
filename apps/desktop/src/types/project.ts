export type ProjectAvailability = "available" | "missing" | "unreadable";
export interface LocalProject {
  id: string;
  path: string;
  name: string;
  lastOpenedAt: number;
  availability: ProjectAvailability;
}
export type PersistenceWarning = "recentProjectsNotSaved";
export interface ProjectSelection {
  project: LocalProject;
  warning: PersistenceWarning | null;
}
export type ProjectThreadStatus =
  | "notLoaded"
  | "idle"
  | "systemError"
  | "active";
export interface ProjectThread {
  id: string;
  name: string | null;
  preview: string;
  createdAt: number;
  updatedAt: number;
  recencyAt: number | null;
  status: ProjectThreadStatus;
  cwd: string;
}
export interface ProjectThreadPage {
  data: ProjectThread[];
  nextCursor: string | null;
}
