import { beforeEach, describe, expect, it, vi } from "vitest";

import type { RivloomIdentity } from "../types/identity";

const tauriMocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: tauriMocks.invoke,
}));

import { getIdentity } from "./identityBridge";

const identity: RivloomIdentity = {
  identityId: "identity-v1-11111111111111111111111111111111",
  displayName: "本机用户",
  deviceId: "device-v1-22222222222222222222222222222222",
  brainMembership: null,
};

describe("identityBridge", () => {
  beforeEach(() => {
    tauriMocks.invoke.mockReset();
    tauriMocks.invoke.mockResolvedValue(identity);
  });

  it("reads identity with only the fixed identity command", async () => {
    await expect(getIdentity()).resolves.toEqual(identity);
    expect(tauriMocks.invoke).toHaveBeenCalledWith("get_identity");
  });
});
