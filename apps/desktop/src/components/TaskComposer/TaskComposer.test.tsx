import userEvent from "@testing-library/user-event";
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { TaskComposer } from "./TaskComposer";

describe("TaskComposer", () => {
  const onSubmit = vi.fn();

  beforeEach(() => {
    onSubmit.mockReset();
    onSubmit.mockResolvedValue(true);
  });

  it("renders the bounded sharing contract without starting a task", () => {
    const { container } = render(
      <TaskComposer available submitting={false} onSubmit={onSubmit} />,
    );

    expect(screen.getByRole("form", { name: "定义本地任务" })).toBeVisible();
    expect(screen.getByText("只发送目标与约束")).toBeVisible();
    expect(onSubmit).not.toHaveBeenCalled();
    expect(container.innerHTML).toMatchSnapshot();
  });

  it("submits normalized content once and clears only after success", async () => {
    const user = userEvent.setup();
    render(<TaskComposer available submitting={false} onSubmit={onSubmit} />);

    await user.type(
      screen.getByLabelText("任务目标"),
      "  修复登录后的状态恢复  ",
    );
    await user.type(
      screen.getByLabelText("执行约束（每行一条）"),
      "  保持存储兼容  {enter}{enter} 不修改 codex-rs  ",
    );
    await user.click(screen.getByRole("button", { name: "启动本地任务" }));

    expect(onSubmit).toHaveBeenCalledOnce();
    expect(onSubmit).toHaveBeenCalledWith("修复登录后的状态恢复", [
      "保持存储兼容",
      "不修改 codex-rs",
    ]);
    expect(screen.getByLabelText("任务目标")).toHaveValue("");
    expect(screen.getByLabelText("执行约束（每行一条）")).toHaveValue("");
  });

  it("keeps the draft when the task was not accepted", async () => {
    const user = userEvent.setup();
    onSubmit.mockResolvedValue(false);
    render(<TaskComposer available submitting={false} onSubmit={onSubmit} />);

    await user.type(screen.getByLabelText("任务目标"), "保留这份草稿");
    await user.click(screen.getByRole("button", { name: "启动本地任务" }));

    expect(screen.getByLabelText("任务目标")).toHaveValue("保留这份草稿");
  });

  it("blocks byte and line limits before calling the backend", async () => {
    const user = userEvent.setup();
    render(<TaskComposer available submitting={false} onSubmit={onSubmit} />);

    fireEvent.change(screen.getByLabelText("任务目标"), {
      target: { value: "你".repeat(1_366) },
    });
    fireEvent.change(screen.getByLabelText("执行约束（每行一条）"), {
      target: {
        value: Array.from({ length: 33 }, (_, index) => `${index}`).join("\n"),
      },
    });

    expect(
      screen.getAllByRole("alert").map(({ textContent }) => textContent),
    ).toEqual(["任务目标不能超过 4 KiB。", "执行约束最多 32 条。"]);
    expect(screen.getByRole("button", { name: "启动本地任务" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "启动本地任务" }));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("mirrors the conservative final Runtime prompt boundary", () => {
    render(<TaskComposer available submitting={false} onSubmit={onSubmit} />);

    fireEvent.change(screen.getByLabelText("任务目标"), {
      target: { value: "a".repeat(995) },
    });

    expect(screen.getByRole("alert")).toHaveTextContent(
      "最终任务正文超过 1,000-byte 安全上限。",
    );
    expect(screen.getByRole("button", { name: "启动本地任务" })).toBeDisabled();
  });

  it("keeps drafting available while the Runtime is offline", async () => {
    const user = userEvent.setup();
    render(
      <TaskComposer available={false} submitting={false} onSubmit={onSubmit} />,
    );

    await user.type(screen.getByLabelText("任务目标"), "离线草稿");

    expect(screen.getByLabelText("任务目标")).toHaveValue("离线草稿");
    expect(screen.getByText("连接 Codex Runtime 后即可启动")).toBeVisible();
    expect(screen.getByRole("button", { name: "启动本地任务" })).toBeDisabled();
  });
});
