import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type {
  CollaborationController,
  CollaborationSnapshot,
} from "../../hooks/useCollaboration";
import { CollaborationCard } from "./CollaborationCard";

function controller(
  snapshot: CollaborationSnapshot | null,
): CollaborationController {
  return {
    snapshot,
    pending: null,
    error: null,
    reload: vi.fn(),
    initialize: vi.fn(),
    start: vi.fn(),
    stop: vi.fn(),
    join: vi.fn(),
    owner: vi.fn(),
    connect: vi.fn(),
    refresh: vi.fn(),
    disconnect: vi.fn(),
    invite: vi.fn(),
    cancel: vi.fn(),
    revoke: vi.fn(),
  };
}
const empty: CollaborationSnapshot = {
  host: { state: "notConfigured" },
  node: {
    state: "notConfigured",
    registration: null,
    binding: null,
    revision: 0,
  },
  directory: null,
};

it("keeps recovery and unknown state read-only and never offers enrollment there", () => {
  const value = controller({
    ...empty,
    node: { ...empty.node, state: "recoveryRequired" },
  });
  const { rerender } = render(
    <CollaborationCard controller={value} identityReady />,
  );
  expect(
    screen.queryByRole("button", { name: "确认信任并加入" }),
  ).not.toBeInTheDocument();
  expect(screen.getByText(/恢复流程尚未开放/)).toMatchSnapshot(
    "recovery cannot re-enroll",
  );
  rerender(
    <CollaborationCard
      controller={{ ...value, snapshot: null, error: "transport" }}
      identityReady
    />,
  );
  expect(screen.getAllByRole("button")).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "读取协作状态" }));
  expect(value.reload).toHaveBeenCalledOnce();
  expect(value.join).not.toHaveBeenCalled();
});

it("requires a deliberate choice of hosting and disables all mutations while busy", () => {
  const value = controller(empty);
  const { rerender } = render(
    <CollaborationCard controller={value} identityReady />,
  );
  expect(
    screen.getByRole("heading", { name: "加入已有 Brain" }),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "托管 Brain" }));
  expect(screen.getByLabelText("私网监听地址与端口")).toHaveValue("");
  expect(value.initialize).not.toHaveBeenCalled();
  rerender(
    <CollaborationCard
      controller={{ ...value, pending: "initialize" }}
      identityReady
    />,
  );
  for (const button of screen.getAllByRole("button"))
    expect(button).toBeDisabled();
  rerender(<CollaborationCard controller={value} identityReady={false} />);
  for (const button of screen.getAllByRole("button"))
    expect(button).toBeDisabled();
});

it("only exposes owner invitation controls after authenticated directory projection", () => {
  const snapshot: CollaborationSnapshot = {
    ...empty,
    node: {
      ...empty.node,
      state: "connected",
      revision: 4,
      binding: {
        brainId: "b",
        memberId: "member-a",
        deviceId: "d",
        nodeId: "n",
      },
    },
    directory: {
      revision: 4,
      entries: [
        {
          type: "member",
          memberId: "member-a",
          displayName: "Alice",
          owner: true,
          revoked: false,
        },
      ],
    },
  };
  const value = controller(snapshot);
  const { rerender } = render(
    <CollaborationCard controller={value} identityReady />,
  );
  expect(screen.getByRole("button", { name: "创建一次性邀请" })).toBeEnabled();
  fireEvent.click(screen.getByRole("button", { name: "刷新连接与目录" }));
  expect(value.refresh).toHaveBeenCalledOnce();
  expect(value.invite).not.toHaveBeenCalled();
  rerender(
    <CollaborationCard
      controller={{
        ...value,
        snapshot: {
          ...snapshot,
          node: { ...snapshot.node, state: "disconnected" },
          directory: null,
        },
      }}
      identityReady
    />,
  );
  expect(
    screen.queryByRole("button", { name: "创建一次性邀请" }),
  ).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "连接已登记 Brain" }));
  expect(value.connect).toHaveBeenCalledOnce();
});
