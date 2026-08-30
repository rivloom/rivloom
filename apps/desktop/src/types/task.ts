export type TaskSpec = {
  goal: string;
  constraints: string[];
};

export type TaskStatus =
  | "draft"
  | "offered"
  | "accepted"
  | "running"
  | "awaitingReview"
  | "approved"
  | "rejected"
  | "cancelled"
  | "failed"
  | "outcomeUnknown";

export type RunStatus =
  | "queued"
  | "running"
  | "waitingApproval"
  | "completed"
  | "cancelled"
  | "failed"
  | "outcomeUnknown";

export type RunRecord = {
  id: string;
  status: RunStatus;
  summary: string | null;
  error: string | null;
};

export type TaskEventKind =
  | { type: "taskStatusChanged"; from: TaskStatus; to: TaskStatus }
  | { type: "runRegistered"; runId: string }
  | {
      type: "runStatusChanged";
      runId: string;
      from: RunStatus;
      to: RunStatus;
    };

export type TaskEvent = {
  sequence: number;
  kind: TaskEventKind;
};

export type TaskRecord = {
  id: string;
  spec: TaskSpec;
  status: TaskStatus;
  summary: string | null;
  error: string | null;
  runs: RunRecord[];
  events: TaskEvent[];
};
