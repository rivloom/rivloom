import userEvent from "@testing-library/user-event";
import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type {
  PatchArtifact,
  RunRecord,
  RunReceipt,
  TaskRecord,
} from "../../types/task";

import { TaskRun } from "./TaskRun";

const receipt: RunReceipt = {
  schemaVersion: 1,
  taskId: "task-a",
  runId: "run-a",
  nodeId: "device-a",
  runtimeId: "codex",
  runtimeVersion: "1.2.3",
  startedAt: 1_788_076_800,
  finishedAt: 1_788_076_860,
  outcome: "success",
  summary: "已修复登录状态恢复。",
  error: null,
  tests: { state: "notReported" },
  patch: {
    baselineCommit: "a".repeat(40),
    state: "complete",
    limitBytes: 524_288,
    byteCount: 24,
    sha256: "b".repeat(64),
  },
  contentSha256: "c".repeat(64),
};

const patch: PatchArtifact = { ...receipt.patch, patch: "+ fixed login\n" };
const baseRun: RunRecord = {
  id: "run-a",
  status: "completed",
  summary: receipt.summary,
  error: null,
  receipt,
};

function run(overrides: Partial<RunRecord> = {}): RunRecord {
  return { ...baseRun, ...overrides };
}

function task(overrides: Partial<TaskRecord> = {}): TaskRecord {
  return {
    id: "task-a",
    spec: {
      goal: "修复登录状态恢复",
      constraints: ["保持存储兼容", "不修改 codex-rs"],
    },
    status: "awaitingReview",
    summary: receipt.summary,
    error: null,
    runs: [run()],
    events: Array.from({ length: 8 }, (_, index) => ({
      sequence: index + 1,
      kind: {
        type: "runStatusChanged" as const,
        runId: "run-a",
        from: "queued" as const,
        to: "running" as const,
      },
    })),
    ...overrides,
  };
}

describe("TaskRun", () => {
  it("shows a bounded receipt and lazily reveals the volatile Patch", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <TaskRun task={task()} patch={patch} stopping={false} onStop={vi.fn()} />,
    );
    const card = screen.getByRole("article", { name: "任务 修复登录状态恢复" });

    expect(within(card).getByText("等待审查")).toBeVisible();
    expect(within(card).getByText("测试未报告")).toBeVisible();
    expect(within(card).queryByText("+ fixed login")).not.toBeInTheDocument();
    expect(within(card).queryByText("#2")).not.toBeInTheDocument();
    expect(within(card).getByText("#3")).toBeVisible();
    expect(container.innerHTML).toMatchSnapshot();

    await user.click(within(card).getByText("查看 Patch"));
    expect(await within(card).findByText("+ fixed login")).toBeVisible();
  });

  it("keeps approval local and interrupts only the exact active run", async () => {
    const user = userEvent.setup();
    const onStop = vi.fn();
    const waiting = task({
      status: "running",
      summary: null,
      runs: [
        run({
          id: "run-waiting",
          status: "waitingApproval",
          summary: null,
          receipt: null,
        }),
      ],
    });
    render(
      <TaskRun task={waiting} patch={null} stopping={false} onStop={onStop} />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(
      "请在本机 Codex 完成审批；远端不能代为批准。",
    );
    await user.click(screen.getByRole("button", { name: "停止这次运行" }));
    expect(onStop).toHaveBeenCalledWith("task-a", "run-waiting");
  });

  it("reports each provided test result without guessing an aggregate", () => {
    const reportedReceipt: RunReceipt = {
      ...receipt,
      tests: {
        state: "reported",
        executions: [
          { name: "cargo test", exitCode: 0 },
          { name: "pnpm test", exitCode: 1 },
        ],
      },
    };
    const reported = task({
      runs: [run({ receipt: reportedReceipt })],
    });
    render(
      <TaskRun
        task={reported}
        patch={null}
        stopping={false}
        onStop={vi.fn()}
      />,
    );

    expect(screen.getByText("cargo test").parentElement).toHaveTextContent(
      "退出码 0通过",
    );
    expect(screen.getByText("pnpm test").parentElement).toHaveTextContent(
      "退出码 1失败",
    );
    expect(screen.queryByText("全部通过")).not.toBeInTheDocument();
  });

  it("makes unknown outcomes explicit and never presents them as success", () => {
    const unknownReceipt: RunReceipt = {
      ...receipt,
      outcome: "outcomeUnknown",
      summary: null,
      error: "The Codex run outcome could not be verified.",
      tests: { state: "notReported" },
      patch: { ...receipt.patch, state: "empty", byteCount: 0, sha256: null },
    };
    const unknown = task({
      status: "outcomeUnknown",
      summary: null,
      error: unknownReceipt.error,
      runs: [
        run({
          status: "outcomeUnknown",
          summary: null,
          error: unknownReceipt.error,
          receipt: unknownReceipt,
        }),
      ],
    });
    render(
      <TaskRun task={unknown} patch={null} stopping={false} onStop={vi.fn()} />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(
      "运行结果无法核实，Rivloom 不会自动重跑。",
    );
    expect(screen.queryByText("运行成功")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "停止这次运行" }),
    ).not.toBeInTheDocument();
  });

  it("shows an oversized Patch as metadata only", () => {
    render(
      <TaskRun
        task={task()}
        patch={{
          ...patch,
          state: "tooLarge",
          byteCount: 600_000,
          sha256: null,
          patch: null,
        }}
        stopping={false}
        onStop={vi.fn()}
      />,
    );

    expect(
      screen.getByText("Patch 超过本地展示上限，仅保留元数据。"),
    ).toBeVisible();
    expect(screen.queryByText("查看 Patch")).not.toBeInTheDocument();
  });
});
