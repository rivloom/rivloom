import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { MemberDirectory } from "../../types/collaboration";
import { MemberPanel } from "./MemberPanel";
const directory: MemberDirectory = {
  revision: 8,
  entries: [
    {
      type: "member",
      memberId: "owner",
      displayName: "Alice",
      owner: true,
      revoked: false,
    },
    {
      type: "member",
      memberId: "bob",
      displayName: "Bob",
      owner: false,
      revoked: false,
    },
    {
      type: "node",
      nodeId: "node-b",
      memberId: "bob",
      online: true,
      lastSeenAt: 1000,
    },
  ],
};
it("requires explicit named confirmation and hides management controls from non-owners", () => {
  const onRevoke = vi.fn().mockResolvedValue(undefined);
  const { rerender } = render(
    <MemberPanel
      directory={directory}
      memberId="owner"
      disabled={false}
      onRevoke={onRevoke}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "撤销 Bob" }));
  expect(onRevoke).not.toHaveBeenCalled();
  expect(screen.getByRole("group", { name: "确认撤销成员" })).toMatchSnapshot(
    "named revocation confirmation",
  );
  fireEvent.click(screen.getByRole("button", { name: "保留成员" }));
  expect(onRevoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "撤销 Bob" }));
  fireEvent.click(screen.getByRole("button", { name: "确认撤销" }));
  expect(onRevoke).toHaveBeenCalledExactlyOnceWith("bob");
  rerender(
    <MemberPanel
      directory={directory}
      memberId="bob"
      disabled={false}
      onRevoke={onRevoke}
    />,
  );
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
  expect(screen.getByText("node-b · 上次对账：在线")).toBeInTheDocument();
});
