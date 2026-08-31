import { expect, it, vi } from "vitest";
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
import {
  collaborationBridge as bridge,
  collaborationError,
} from "./collaborationBridge";

it("only invokes explicit desktop collaboration commands with exact envelopes", async () => {
  expect(invoke).not.toHaveBeenCalled();
  const params = {
    descriptor: "public",
    confirmedFingerprint: "fingerprint",
    invitation: {
      brainId: "brain-a",
      invitationId: "invite-a",
      expiresAt: 100,
      secret: "transient",
    },
  };
  await bridge.host();
  await bridge.initialize({
    address: "127.0.0.1:7443",
    serverName: "localhost",
  });
  await bridge.start();
  await bridge.stop();
  await bridge.node();
  await bridge.join(params);
  await bridge.owner("confirmed");
  await bridge.connect();
  await bridge.refresh();
  await bridge.disconnect();
  await bridge.members();
  await bridge.invite();
  await bridge.cancel("invite-a");
  await bridge.revoke("member-a");
  expect(invoke.mock.calls).toEqual([
    ["get_local_brain_status"],
    [
      "initialize_local_brain",
      { params: { address: "127.0.0.1:7443", serverName: "localhost" } },
    ],
    ["start_local_brain"],
    ["stop_local_brain"],
    ["get_node_status"],
    ["join_brain", { params }],
    ["connect_brain_owner", { params: { confirmedFingerprint: "confirmed" } }],
    ["connect_brain"],
    ["refresh_brain"],
    ["disconnect_brain"],
    ["list_brain_members"],
    ["create_brain_invitation"],
    ["cancel_brain_invitation", { params: { invitationId: "invite-a" } }],
    ["revoke_brain_member", { params: { memberId: "member-a" } }],
  ]);
});

it("reduces arbitrary transport details to bounded errors without retrying", async () => {
  invoke.mockClear().mockRejectedValue(new Error("private transport details"));
  await expect(bridge.connect()).rejects.toThrow("private transport details");
  expect(invoke).toHaveBeenCalledOnce();
  expect(collaborationError(new Error("private transport details"))).toBe(
    "unavailable",
  );
  expect(collaborationError({ secret: "private" })).toBe("unavailable");
  expect(collaborationError("credential")).toBe("credential");
});
