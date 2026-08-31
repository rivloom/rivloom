import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { InvitationPanel } from "./InvitationPanel";
const invitation = {
  brainId: "brain-a",
  invitationId: "a".repeat(64),
  secret: "b".repeat(64),
  expiresAt: 1500,
};
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it("creates once, hides secret on visibility change, and still permits explicit revocation", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(1000_000);
  let resolve!: (value: typeof invitation) => void;
  const onInvite = vi.fn().mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  const onCancel = vi.fn().mockResolvedValue(undefined);
  render(
    <InvitationPanel
      disabled={false}
      onInvite={onInvite}
      onCancel={onCancel}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" }));
  fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" }));
  await act(async () => resolve(invitation));
  expect(onInvite).toHaveBeenCalledOnce();
  expect(screen.getByLabelText("一次性邀请（仅本次显示）")).toHaveValue(
    JSON.stringify(invitation),
  );
  vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  fireEvent(document, new Event("visibilitychange"));
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  await act(async () =>
    fireEvent.click(screen.getByRole("button", { name: "撤销这份邀请" })),
  );
  expect(onCancel).toHaveBeenCalledExactlyOnceWith(invitation.invitationId);
  expect(screen.queryByText(/邀请剩余/)).not.toBeInTheDocument();
});

it("expires the display and never reissues; errors cannot echo a secret", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(1499_000);
  const onInvite = vi.fn().mockResolvedValue(invitation);
  render(
    <InvitationPanel disabled={false} onInvite={onInvite} onCancel={vi.fn()} />,
  );
  await act(async () =>
    fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" })),
  );
  await act(async () => vi.advanceTimersByTime(1000));
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(onInvite).toHaveBeenCalledOnce();
  onInvite.mockRejectedValue(new Error("private secret"));
  await act(async () =>
    fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" })),
  );
  expect(screen.getByRole("alert")).toMatchSnapshot(
    "uncertain invitation does not echo errors",
  );
});

it("does not display a late secret after hiding while creation is pending", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(1000_000);
  let resolve!: (value: typeof invitation) => void;
  render(
    <InvitationPanel
      disabled={false}
      onInvite={() =>
        new Promise((done) => {
          resolve = done;
        })
      }
      onCancel={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" }));
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  fireEvent(document, new Event("visibilitychange"));
  hidden.mockReturnValue(false);
  await act(async () => resolve(invitation));
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "撤销这份邀请" })).toBeEnabled();
});
