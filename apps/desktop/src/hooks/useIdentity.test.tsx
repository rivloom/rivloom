import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { RivloomIdentity } from "../types/identity";

const bridgeMocks = vi.hoisted(() => ({ getIdentity: vi.fn() }));

vi.mock("../lib/identityBridge", () => bridgeMocks);

import { useIdentity } from "./useIdentity";

const identity: RivloomIdentity = {
  identityId: "identity-v1-11111111111111111111111111111111",
  displayName: "本机用户",
  deviceId: "device-v1-22222222222222222222222222222222",
  brainMembership: null,
};

describe("useIdentity", () => {
  beforeEach(() => {
    bridgeMocks.getIdentity.mockReset();
    bridgeMocks.getIdentity.mockResolvedValue(identity);
  });

  it("loads local identity without depending on Runtime auth", async () => {
    const { result } = renderHook(() => useIdentity());

    await waitFor(() =>
      expect(result.current.state).toEqual({ state: "ready", identity }),
    );
    expect(bridgeMocks.getIdentity).toHaveBeenCalledOnce();
  });

  it("can retry a failed initial read", async () => {
    bridgeMocks.getIdentity.mockRejectedValueOnce(new Error("offline"));
    const { result } = renderHook(() => useIdentity());

    await waitFor(() => expect(result.current.state.state).toBe("error"));
    await act(async () => result.current.refresh());
    expect(result.current.state).toEqual({ state: "ready", identity });
    expect(result.current.pendingAction).toBeNull();
  });
});
