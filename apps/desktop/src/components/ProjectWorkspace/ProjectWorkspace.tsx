import { zhCN } from "../../content/zh-CN";
import { useTasks } from "../../hooks/useTasks";
import type { LocalProject } from "../../types/project";
import type { TaskCommandError } from "../../types/task";
import { TaskComposer } from "../TaskComposer/TaskComposer";
import { TaskRun } from "../TaskRun/TaskRun";

import styles from "./ProjectWorkspace.module.css";

type ProjectWorkspaceProps = {
  project: LocalProject;
  runtimeConnected: boolean;
  onBack: () => void;
};

function errorMessage(error: TaskCommandError) {
  return zhCN.task.errors[error];
}

export function ProjectWorkspace({
  project,
  runtimeConnected,
  onBack,
}: ProjectWorkspaceProps) {
  const tasks = useTasks(project.id, runtimeConnected);
  const starting = tasks.action?.type === "start";

  const handleStart = async (goal: string, constraints: string[]) =>
    (await tasks.startTask(goal, constraints)) !== null;

  return (
    <section
      className={styles.workspace}
      aria-label={zhCN.workspace.label(project.name)}
    >
      <header className={styles.header}>
        <button className={styles.backButton} type="button" onClick={onBack}>
          <span aria-hidden="true">←</span>
          {zhCN.workspace.actions.back}
        </button>
        <div className={styles.heading}>
          <p>{zhCN.workspace.eyebrow}</p>
          <h2 id="project-workspace-title">{project.name}</h2>
          <span>
            <strong>{zhCN.workspace.pathLabel}</strong>
            <code>{project.path}</code>
          </span>
        </div>
      </header>

      {!runtimeConnected ? (
        <div className={styles.disconnected} role="alert">
          <span aria-hidden="true">!</span>
          <div>
            <strong>{zhCN.workspace.disconnected.title}</strong>
            <p>{zhCN.workspace.disconnected.description}</p>
          </div>
        </div>
      ) : null}

      <div className={styles.content}>
        <TaskComposer
          available={runtimeConnected}
          submitting={starting}
          onSubmit={handleStart}
        />

        {tasks.actionError ? (
          <p className={styles.actionError} role="alert">
            {errorMessage(tasks.actionError)}
          </p>
        ) : null}

        <section
          className={styles.taskSection}
          aria-labelledby="local-task-list-title"
        >
          <header className={styles.taskSectionHeader}>
            <div>
              <p>{zhCN.task.list.eyebrow}</p>
              <h3 id="local-task-list-title">{zhCN.task.list.title}</h3>
              <span>{zhCN.task.list.description}</span>
            </div>
            <strong>{zhCN.task.list.count(tasks.tasks.length)}</strong>
          </header>

          {tasks.state.state === "loading" ? (
            <div className={styles.taskState} role="status">
              <strong>{zhCN.task.list.loading.title}</strong>
              <p>{zhCN.task.list.loading.description}</p>
            </div>
          ) : null}
          {tasks.state.state === "error" ? (
            <div className={styles.stateError} role="alert">
              <strong>{zhCN.task.list.error.title}</strong>
              <p>{errorMessage(tasks.state.error)}</p>
            </div>
          ) : null}
          {tasks.state.state === "empty" ? (
            <div className={styles.taskState}>
              <strong>{zhCN.task.list.empty.title}</strong>
              <p>{zhCN.task.list.empty.description}</p>
            </div>
          ) : null}

          {tasks.tasks.length > 0 ? (
            <div className={styles.taskList}>
              {tasks.tasks.map((task) => (
                <TaskRun
                  key={task.id}
                  task={task}
                  patch={tasks.patches[task.id] ?? null}
                  stopping={
                    tasks.action?.type === "stop" &&
                    tasks.action.taskId === task.id
                  }
                  onStop={(taskId, runId) => {
                    void tasks.stopTask(taskId, runId);
                  }}
                />
              ))}
            </div>
          ) : null}
        </section>
      </div>
    </section>
  );
}
