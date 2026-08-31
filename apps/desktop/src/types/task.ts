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

export type PatchArtifactState =
  | "empty"
  | "complete"
  | "tooLarge"
  | "unsupportedEncoding";

export type PatchArtifactMetadata = {
  baselineCommit: string;
  state: PatchArtifactState;
  limitBytes: number;
  byteCount: number | null;
  sha256: string | null;
};

export type PatchArtifact = PatchArtifactMetadata & {
  patch: string | null;
};

export type RunReceiptOutcome =
  | "success"
  | "failed"
  | "cancelled"
  | "outcomeUnknown";

export type TestExecution = {
  name: string;
  exitCode: number;
};

export type TestReport =
  | { state: "notReported" }
  | { state: "reported"; executions: TestExecution[] };

export type RunReceipt = {
  schemaVersion: number;
  taskId: string;
  runId: string;
  nodeId: string;
  runtimeId: string;
  runtimeVersion: string;
  startedAt: number;
  finishedAt: number;
  outcome: RunReceiptOutcome;
  summary: string | null;
  error: string | null;
  tests: TestReport;
  patch: PatchArtifactMetadata;
  contentSha256: string;
};

export type RunRecord = {
  id: string;
  status: RunStatus;
  summary: string | null;
  error: string | null;
  receipt: RunReceipt | null;
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

export type LocalTaskRun = {
  task: TaskRecord;
  runId: string;
};

export type LocalTaskUpdate = {
  projectId: string;
  task: TaskRecord;
  patch: PatchArtifact | null;
};

export type TaskCommandError =
  | "invalidTask"
  | "taskUnavailable"
  | "projectUnavailable"
  | "identityUnavailable"
  | "runtimeUnavailable"
  | "runUnavailable"
  | "taskCapacityReached";
