import { useState } from "react";

import { zhCN } from "../../content/zh-CN";
import type {
  PatchArtifact,
  RunStatus,
  TaskEventKind,
  TaskRecord,
  TaskStatus,
} from "../../types/task";

import styles from "./TaskRun.module.css";

const MAX_VISIBLE_EVENTS = 6;

type TaskRunProps = {
  task: TaskRecord;
  patch: PatchArtifact | null;
  stopping: boolean;
  onStop: (taskId: string, runId: string) => void | Promise<void>;
};

const taskStatus: Record<TaskStatus, string> = zhCN.task.run.taskStatus;
const runStatus: Record<RunStatus, string> = zhCN.task.run.runStatus;

function toneOf(status: TaskStatus) {
  switch (status) {
    case "approved":
      return styles.success;
    case "failed":
    case "rejected":
    case "outcomeUnknown":
      return styles.danger;
    case "running":
    case "awaitingReview":
      return styles.accent;
    case "draft":
    case "offered":
    case "accepted":
    case "cancelled":
      return styles.muted;
  }
}

function eventLabel(kind: TaskEventKind) {
  switch (kind.type) {
    case "taskStatusChanged":
      return zhCN.task.run.timeline.taskChanged(
        taskStatus[kind.from],
        taskStatus[kind.to],
      );
    case "runRegistered":
      return zhCN.task.run.timeline.registered;
    case "runStatusChanged":
      return zhCN.task.run.timeline.runChanged(
        runStatus[kind.from],
        runStatus[kind.to],
      );
  }
}

function formatBytes(value: number | null) {
  return value === null
    ? zhCN.task.run.patch.sizeUnavailable
    : zhCN.task.run.patch.bytes(value.toLocaleString("zh-CN"));
}

export function TaskRun({ task, patch, stopping, onStop }: TaskRunProps) {
  const [patchOpen, setPatchOpen] = useState(false);
  const run = task.runs.at(-1) ?? null;
  const receipt = run?.receipt ?? null;
  const artifact =
    patch ?? (receipt ? { ...receipt.patch, patch: null } : null);
  const canStop =
    run?.status === "running" || run?.status === "waitingApproval";
  const events = task.events.slice(-MAX_VISIBLE_EVENTS);
  const titleId = `task-title-${task.id}`;

  return (
    <article
      className={styles.run}
      aria-label={zhCN.task.run.label(task.spec.goal)}
    >
      <header className={styles.header}>
        <div className={styles.heading}>
          <p>{zhCN.task.run.eyebrow}</p>
          <h3 id={titleId}>{task.spec.goal}</h3>
          {task.spec.constraints.length > 0 ? (
            <ul
              className={styles.constraints}
              aria-label={zhCN.task.run.constraints}
            >
              {task.spec.constraints.map((constraint, index) => (
                <li key={`${index}-${constraint}`}>{constraint}</li>
              ))}
            </ul>
          ) : null}
        </div>
        <div className={styles.headerActions}>
          <span className={`${styles.status} ${toneOf(task.status)}`}>
            {taskStatus[task.status]}
          </span>
          {canStop && run ? (
            <button
              type="button"
              disabled={stopping}
              aria-busy={stopping}
              onClick={() => void onStop(task.id, run.id)}
            >
              {stopping
                ? zhCN.task.run.stop.stopping
                : zhCN.task.run.stop.action}
            </button>
          ) : null}
        </div>
      </header>

      {run?.status === "waitingApproval" ? (
        <div className={`${styles.notice} ${styles.approval}`} role="alert">
          <p>{zhCN.task.run.notices.waitingApproval}</p>
        </div>
      ) : null}
      {task.status === "outcomeUnknown" ? (
        <div className={`${styles.notice} ${styles.unknown}`} role="alert">
          <p>{zhCN.task.run.notices.outcomeUnknown}</p>
        </div>
      ) : null}

      <div className={styles.body}>
        <section
          className={styles.timeline}
          aria-labelledby={`${titleId}-timeline`}
        >
          <div className={styles.sectionHeading}>
            <p>{zhCN.task.run.timeline.eyebrow}</p>
            <h4 id={`${titleId}-timeline`}>{zhCN.task.run.timeline.title}</h4>
          </div>
          <ol>
            {events.map((event) => (
              <li key={event.sequence}>
                <code>#{event.sequence}</code>
                <span>{eventLabel(event.kind)}</span>
              </li>
            ))}
          </ol>
        </section>

        <section
          className={styles.receipt}
          aria-labelledby={`${titleId}-receipt`}
        >
          <div className={styles.sectionHeading}>
            <p>{zhCN.task.run.receipt.eyebrow}</p>
            <h4 id={`${titleId}-receipt`}>{zhCN.task.run.receipt.title}</h4>
            {run ? <span>{runStatus[run.status]}</span> : null}
          </div>

          {receipt ? (
            <div className={styles.receiptContent}>
              <div className={styles.summary}>
                <strong>{zhCN.task.run.receipt.summary}</strong>
                <p>
                  {receipt.summary ??
                    receipt.error ??
                    zhCN.task.run.receipt.summaryUnavailable}
                </p>
                <small>
                  {zhCN.task.run.receipt.runtime(
                    receipt.runtimeId,
                    receipt.runtimeVersion,
                  )}
                </small>
              </div>

              <div className={styles.tests}>
                <strong>{zhCN.task.run.tests.title}</strong>
                {receipt.tests.state === "notReported" ? (
                  <p className={styles.notReported}>
                    {zhCN.task.run.tests.notReported}
                  </p>
                ) : (
                  <ul>
                    {receipt.tests.executions.map((execution, index) => (
                      <li key={`${index}-${execution.name}`}>
                        <span>{execution.name}</span>
                        <small>
                          {zhCN.task.run.tests.exitCode(execution.exitCode)}
                        </small>
                        <b
                          className={
                            execution.exitCode === 0
                              ? styles.testPassed
                              : styles.testFailed
                          }
                        >
                          {execution.exitCode === 0
                            ? zhCN.task.run.tests.passed
                            : zhCN.task.run.tests.failed}
                        </b>
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              {artifact ? (
                <div className={styles.patch}>
                  <div className={styles.patchHeading}>
                    <strong>{zhCN.task.run.patch.title}</strong>
                    <span>{formatBytes(artifact.byteCount)}</span>
                  </div>
                  <p>{zhCN.task.run.patch.state[artifact.state]}</p>
                  {artifact.sha256 ? (
                    <code title={artifact.sha256}>
                      sha256:{artifact.sha256.slice(0, 12)}…
                    </code>
                  ) : null}
                  {artifact.state === "complete" && artifact.patch ? (
                    <details
                      onToggle={(event) =>
                        setPatchOpen(event.currentTarget.open)
                      }
                    >
                      <summary>{zhCN.task.run.patch.open}</summary>
                      {patchOpen ? <pre>{artifact.patch}</pre> : null}
                    </details>
                  ) : null}
                </div>
              ) : null}
            </div>
          ) : (
            <p className={styles.pending}>{zhCN.task.run.receipt.pending}</p>
          )}
        </section>
      </div>
    </article>
  );
}
