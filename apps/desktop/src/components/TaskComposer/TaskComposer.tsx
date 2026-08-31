import { useMemo, useState, type FormEvent } from "react";

import { zhCN } from "../../content/zh-CN";

import styles from "./TaskComposer.module.css";

const MAX_GOAL_BYTES = 4 * 1024;
const MAX_CONSTRAINTS = 32;
const MAX_CONSTRAINT_BYTES = 1024;
const MAX_CONSTRAINT_TOTAL_BYTES = 8 * 1024;
const MAX_RUN_PROMPT_BYTES = 1_000;

type TaskComposerProps = {
  available: boolean;
  submitting: boolean;
  onSubmit: (goal: string, constraints: string[]) => boolean | Promise<boolean>;
};

function bytes(value: string) {
  return new TextEncoder().encode(value).byteLength;
}

function constraintsOf(value: string) {
  return value
    .split("\n")
    .map((constraint) => constraint.trim())
    .filter(Boolean);
}

export function TaskComposer({
  available,
  submitting,
  onSubmit,
}: TaskComposerProps) {
  const [goal, setGoal] = useState("");
  const [constraintDraft, setConstraintDraft] = useState("");
  const [pending, setPending] = useState(false);
  const normalizedGoal = goal.trim();
  const constraints = useMemo(
    () => constraintsOf(constraintDraft),
    [constraintDraft],
  );
  const goalBytes = bytes(normalizedGoal);
  const constraintBytes = constraints.map(bytes);
  const constraintTotalBytes = constraintBytes.reduce(
    (total, length) => total + length,
    0,
  );
  const prompt = `Goal:\n${normalizedGoal}${
    constraints.length > 0
      ? `\n\nConstraints:${constraints
          .map((constraint) => `\n- ${constraint}`)
          .join("")}`
      : ""
  }`;
  const promptBytes = bytes(prompt);
  const storageErrors = [
    goalBytes > MAX_GOAL_BYTES ? zhCN.task.composer.errors.goalTooLarge : null,
    constraints.length > MAX_CONSTRAINTS
      ? zhCN.task.composer.errors.tooManyConstraints
      : null,
    constraintBytes.some((length) => length > MAX_CONSTRAINT_BYTES)
      ? zhCN.task.composer.errors.constraintTooLarge
      : null,
    constraintTotalBytes > MAX_CONSTRAINT_TOTAL_BYTES
      ? zhCN.task.composer.errors.constraintsTooLarge
      : null,
  ].filter((error) => error !== null);
  const errors = [
    ...storageErrors,
    storageErrors.length === 0 && promptBytes > MAX_RUN_PROMPT_BYTES
      ? zhCN.task.composer.errors.promptTooLarge
      : null,
  ].filter((error) => error !== null);
  const busy = submitting || pending;
  const blocked =
    !available || busy || normalizedGoal.length === 0 || errors.length > 0;

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (blocked) return;
    setPending(true);
    try {
      if (await onSubmit(normalizedGoal, constraints)) {
        setGoal("");
        setConstraintDraft("");
      }
    } catch {
      // The parent owns sanitized task errors; keep this local draft intact.
    } finally {
      setPending(false);
    }
  };

  return (
    <form
      className={styles.composer}
      aria-label={zhCN.task.composer.label}
      onSubmit={(event) => void handleSubmit(event)}
    >
      <header className={styles.header}>
        <span className={styles.step} aria-hidden="true">
          01
        </span>
        <div>
          <p className={styles.eyebrow}>{zhCN.task.composer.eyebrow}</p>
          <h3>{zhCN.task.composer.title}</h3>
          <p className={styles.description}>{zhCN.task.composer.description}</p>
        </div>
        <span className={styles.boundary}>
          {zhCN.task.composer.boundary(promptBytes, MAX_RUN_PROMPT_BYTES)}
        </span>
      </header>

      <div className={styles.fields}>
        <div className={styles.field}>
          <span className={styles.fieldHeading}>
            <label htmlFor="local-task-goal">
              <strong>{zhCN.task.composer.goal.label}</strong>
            </label>
            <span>
              {zhCN.task.composer.goal.bytes(goalBytes, MAX_GOAL_BYTES)}
            </span>
          </span>
          <textarea
            id="local-task-goal"
            value={goal}
            rows={5}
            disabled={busy}
            placeholder={zhCN.task.composer.goal.placeholder}
            onChange={(event) => setGoal(event.currentTarget.value)}
          />
          <small>{zhCN.task.composer.goal.hint}</small>
        </div>

        <div className={styles.field}>
          <span className={styles.fieldHeading}>
            <label htmlFor="local-task-constraints">
              <strong>{zhCN.task.composer.constraints.label}</strong>
            </label>
            <span>
              {zhCN.task.composer.constraints.count(
                constraints.length,
                MAX_CONSTRAINTS,
              )}
            </span>
          </span>
          <textarea
            id="local-task-constraints"
            value={constraintDraft}
            rows={4}
            disabled={busy}
            placeholder={zhCN.task.composer.constraints.placeholder}
            onChange={(event) => setConstraintDraft(event.currentTarget.value)}
          />
          <small>{zhCN.task.composer.constraints.hint}</small>
        </div>
      </div>

      {errors.length > 0 ? (
        <div className={styles.errors}>
          {errors.map((error) => (
            <p role="alert" key={error}>
              {error}
            </p>
          ))}
        </div>
      ) : null}

      <footer className={styles.footer}>
        <div className={styles.sharing}>
          <span aria-hidden="true">↗</span>
          <div aria-live="polite">
            <strong>{zhCN.task.composer.sharing.title}</strong>
            <p>
              {available
                ? zhCN.task.composer.sharing.description
                : zhCN.task.composer.sharing.runtimeRequired}
            </p>
          </div>
        </div>
        <button type="submit" disabled={blocked} aria-busy={busy}>
          <span aria-hidden="true">▶</span>
          {busy
            ? zhCN.task.composer.actions.starting
            : zhCN.task.composer.actions.start}
        </button>
      </footer>
    </form>
  );
}
