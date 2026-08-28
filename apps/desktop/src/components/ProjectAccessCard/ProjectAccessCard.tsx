import { zhCN } from "../../content/zh-CN";
import type {
  RecentProjectAction,
  RecentProjectsState,
} from "../../hooks/useRecentProjects";
import type {
  LocalProject,
  PersistenceWarning,
  ProjectSelection,
} from "../../types/project";

import styles from "./ProjectAccessCard.module.css";

type ProjectAccessCardProps = {
  state: RecentProjectsState;
  pendingAction: RecentProjectAction | null;
  warning: PersistenceWarning | null;
  activeProjectId: string | null;
  onRefresh: () => void;
  onSelect: () => Promise<ProjectSelection | null>;
  onOpenProject: (project: LocalProject) => void;
  onRemove: (projectId: string) => void;
};

export function ProjectAccessCard({
  state,
  pendingAction,
  warning,
  activeProjectId,
  onRefresh,
  onSelect,
  onOpenProject,
  onRemove,
}: ProjectAccessCardProps) {
  const projects = projectsOf(state);
  const busy = pendingAction !== null;
  const selectPending = pendingAction?.type === "select";

  const handleSelect = async () => {
    const selection = await onSelect();
    if (selection) onOpenProject(selection.project);
  };

  return (
    <section className={styles.card} aria-labelledby="project-access-title">
      <div className={styles.accent} aria-hidden="true" />
      <header className={styles.header}>
        <div>
          <p className={styles.eyebrow}>{zhCN.project.eyebrow}</p>
          <h2 id="project-access-title" className={styles.title}>
            {zhCN.project.title}
          </h2>
          <p className={styles.description}>{zhCN.project.description}</p>
        </div>
        <button
          className={styles.primaryButton}
          type="button"
          onClick={() => void handleSelect()}
          disabled={busy || state.state === "loading"}
          aria-busy={selectPending || undefined}
        >
          <span aria-hidden="true">+</span>
          {zhCN.project.actions.select}
        </button>
      </header>

      {warning ? (
        <p className={styles.warning} role="status">
          <span aria-hidden="true">!</span>
          {zhCN.project.warning.recentProjectsNotSaved}
        </p>
      ) : null}

      {state.state === "error" ? (
        <div className={styles.errorPanel} role="alert">
          <p>{state.message}</p>
          <button type="button" onClick={onRefresh} disabled={busy}>
            {zhCN.project.actions.refresh}
          </button>
        </div>
      ) : null}

      <div className={styles.body} aria-live="polite">
        <div className={styles.listHeading}>
          <div>
            <h3>{zhCN.project.recent.title}</h3>
            <p>{zhCN.project.recent.description}</p>
          </div>
          {projects.length > 0 ? (
            <span>{zhCN.project.recent.count(projects.length)}</span>
          ) : null}
        </div>

        {state.state === "loading" ? (
          <ProjectPlaceholder mark="···" {...zhCN.project.loading} />
        ) : null}
        {state.state === "empty" ||
        (state.state === "error" && projects.length === 0) ? (
          <ProjectPlaceholder mark="↗" {...zhCN.project.empty} />
        ) : null}
        {projects.length > 0 ? (
          <ul className={styles.projectList}>
            {projects.map((project) => (
              <ProjectRow
                key={project.id}
                project={project}
                active={
                  project.availability === "available" &&
                  activeProjectId === project.id
                }
                busy={busy}
                removing={
                  pendingAction?.type === "remove" &&
                  pendingAction.projectId === project.id
                }
                onOpenProject={onOpenProject}
                onRemove={onRemove}
              />
            ))}
          </ul>
        ) : null}
      </div>
    </section>
  );
}

function ProjectRow({
  project,
  active,
  busy,
  removing,
  onOpenProject,
  onRemove,
}: {
  project: LocalProject;
  active: boolean;
  busy: boolean;
  removing: boolean;
  onOpenProject: (project: LocalProject) => void;
  onRemove: (projectId: string) => void;
}) {
  const available = project.availability === "available";
  const availability =
    project.availability === "available"
      ? null
      : zhCN.project.availability[project.availability];

  return (
    <li className={`${styles.projectRow} ${active ? styles.activeRow : ""}`}>
      <button
        className={styles.projectButton}
        type="button"
        aria-label={zhCN.project.actions.open(project.name)}
        onClick={() => onOpenProject(project)}
        disabled={!available || busy}
      >
        <span className={styles.folderMark} aria-hidden="true">
          {available ? "↗" : "—"}
        </span>
        <span className={styles.projectCopy}>
          <span className={styles.nameLine}>
            <strong>{project.name}</strong>
            {active ? (
              <span className={styles.activeLabel}>
                {zhCN.project.activeLabel}
              </span>
            ) : null}
            {availability ? (
              <span className={styles.availability}>{availability}</span>
            ) : null}
          </span>
          <span className={styles.path}>{project.path}</span>
          <span className={styles.openedAt}>
            {zhCN.project.recent.lastOpened}{" "}
            <time
              dateTime={new Date(project.lastOpenedAt * 1000).toISOString()}
            >
              {formatLastOpened(project.lastOpenedAt)}
            </time>
          </span>
        </span>
      </button>
      <button
        className={styles.removeButton}
        type="button"
        aria-label={zhCN.project.actions.remove(project.name)}
        aria-busy={removing || undefined}
        onClick={() => onRemove(project.id)}
        disabled={busy}
      >
        {zhCN.project.actions.removeShort}
      </button>
    </li>
  );
}

function ProjectPlaceholder({
  mark,
  title,
  description,
}: {
  mark: string;
  title: string;
  description: string;
}) {
  return (
    <div className={styles.placeholder}>
      <span aria-hidden="true">{mark}</span>
      <div>
        <strong>{title}</strong>
        <p>{description}</p>
      </div>
    </div>
  );
}

function projectsOf(state: RecentProjectsState) {
  return state.state === "ready" || state.state === "error"
    ? state.projects
    : [];
}

function formatLastOpened(timestamp: number) {
  return new Date(timestamp * 1000).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}
