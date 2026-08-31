import { StrictMode } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { CollaborationBridge } from "../lib/collaborationBridge";
import type { NodeStatus } from "../types/collaboration";
import { useCollaboration } from "./useCollaboration";

const empty: NodeStatus = {
  state: "notConfigured",
  registration: null,
  binding: null,
  revision: 0,
};
function fixture() {
  return {
    host: vi.fn().mockResolvedValue({ state: "notConfigured" }),
    node: vi.fn().mockResolvedValue(empty),
    members: vi.fn().mockResolvedValue({ revision: 4, entries: [] }),
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
  } satisfies CollaborationBridge;
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { resolve, promise };
}

it("does nothing before local identity is ready, then reads without connecting even under StrictMode", async () => {
  const bridge = fixture();
  const { result, rerender } = renderHook(
    ({ enabled }) => useCollaboration(enabled, bridge),
    {
      initialProps: { enabled: false },
      wrapper: StrictMode,
    },
  );
  expect(bridge.host).not.toHaveBeenCalled();
  rerender({ enabled: true });
  await waitFor(() =>
    expect(result.current.snapshot).toEqual({
      host: { state: "notConfigured" },
      node: empty,
      directory: null,
    }),
  );
  expect(bridge.connect).not.toHaveBeenCalled();
  expect(bridge.members).not.toHaveBeenCalled();
});

it("serializes mutations immediately and reads the completed projection after success", async () => {
  const bridge = fixture();
  const operation = deferred<NodeStatus>();
  bridge.connect.mockReturnValue(operation.promise);
  const { result } = renderHook(() => useCollaboration(true, bridge));
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  let first!: Promise<NodeStatus | null>;
  act(() => {
    first = result.current.connect();
    void result.current.connect();
    void result.current.invite();
  });
  expect(bridge.connect).toHaveBeenCalledOnce();
  expect(bridge.invite).not.toHaveBeenCalled();
  bridge.node.mockResolvedValue({ ...empty, state: "connected", revision: 4 });
  await act(async () => {
    operation.resolve(empty);
    await first;
  });
  expect(result.current.snapshot?.directory).toEqual({
    revision: 4,
    entries: [],
  });
  expect(result.current.pending).toBeNull();
});

it("preserves uncertain join failure, discovers recoveryRequired, and never resends", async () => {
  const bridge = fixture();
  bridge.join.mockRejectedValue("transport");
  const { result } = renderHook(() => useCollaboration(true, bridge));
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  bridge.node.mockResolvedValue({ ...empty, state: "recoveryRequired" });
  await act(async () => {
    await result.current.join({
      descriptor: "public",
      confirmedFingerprint: "confirmed",
      invitation: {
        brainId: "b",
        invitationId: "i",
        secret: "private",
        expiresAt: 100,
      },
    });
  });
  expect(result.current.error).toBe("transport");
  expect(result.current.snapshot?.node.state).toBe("recoveryRequired");
  await act(async () => {
    await result.current.reload();
  });
  expect(bridge.join).toHaveBeenCalledOnce();
  expect(JSON.stringify(result.current)).not.toContain("private");
});

it("fails closed on status-read errors and only allows explicit read until status is known", async () => {
  const bridge = fixture();
  const { result } = renderHook(() => useCollaboration(true, bridge));
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  bridge.node.mockRejectedValue(new Error("raw detail"));
  await act(async () => {
    await result.current.refresh();
  });
  expect(result.current.snapshot).toBeNull();
  expect(result.current.error).toBe("unavailable");
  await act(async () => {
    await result.current.initialize({ address: "address", serverName: "name" });
  });
  expect(bridge.initialize).not.toHaveBeenCalled();
  bridge.node.mockResolvedValue(empty);
  await act(async () => {
    await result.current.reload();
  });
  expect(result.current.snapshot?.node).toEqual(empty);
});

it("discards late invitation results after unmount, without retry or persistence", async () => {
  const bridge = fixture();
  const invitation = deferred<unknown>();
  bridge.invite.mockReturnValue(invitation.promise);
  const { result, unmount } = renderHook(() => useCollaboration(true, bridge));
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  let response!: ReturnType<typeof result.current.invite>;
  act(() => {
    response = result.current.invite();
  });
  unmount();
  invitation.resolve({ secret: "private" });
  await expect(response).resolves.toBeNull();
  expect(bridge.invite).toHaveBeenCalledOnce();
});

it("does not let a slow initial read replace an explicit newer read", async () => {
  const bridge = fixture();
  const initial = deferred<NodeStatus>();
  bridge.node.mockReturnValueOnce(initial.promise);
  const { result } = renderHook(() => useCollaboration(true, bridge));
  bridge.node.mockResolvedValue({
    ...empty,
    state: "disconnected",
    revision: 8,
  });
  await act(async () => {
    await result.current.reload();
  });
  await act(async () => {
    initial.resolve(empty);
  });
  expect(result.current.snapshot?.node).toEqual({
    ...empty,
    state: "disconnected",
    revision: 8,
  });
});

it("clears pending after an identity availability change without returning an old secret", async () => {
  const bridge = fixture();
  const operation = deferred<unknown>();
  bridge.invite.mockReturnValue(operation.promise);
  const { result, rerender } = renderHook(
    ({ enabled }) => useCollaboration(enabled, bridge),
    { initialProps: { enabled: true } },
  );
  await waitFor(() => expect(result.current.snapshot).not.toBeNull());
  let response!: ReturnType<typeof result.current.invite>;
  act(() => {
    response = result.current.invite();
  });
  rerender({ enabled: false });
  rerender({ enabled: true });
  await act(async () => {
    operation.resolve({ secret: "old" });
    await response;
  });
  expect(result.current.pending).toBeNull();
  await expect(response).resolves.toBeNull();
});
