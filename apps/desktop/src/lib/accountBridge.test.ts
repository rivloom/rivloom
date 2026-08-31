import { beforeEach, describe, expect, it, vi } from "vitest";

import type { CodexRuntimeAuthStatus } from "../types/account";

const tauriMocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: tauriMocks.invoke,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: tauriMocks.listen,
}));

import {
  cancelAccountLogin,
  getAccountStatus,
  logoutAccount,
  onAccountStatusChanged,
  startChatgptLogin,
} from "./accountBridge";

const signedOutStatus: CodexRuntimeAuthStatus = { state: "signedOut" };

describe("accountBridge", () => {
  beforeEach(() => {
    tauriMocks.invoke.mockReset();
    tauriMocks.listen.mockReset();
  });

  it.each([
    ["get_account_status", getAccountStatus],
    ["start_chatgpt_login", startChatgptLogin],
    ["cancel_account_login", cancelAccountLogin],
    ["logout_account", logoutAccount],
  ])("invokes only the fixed %s command", async (command, call) => {
    tauriMocks.invoke.mockResolvedValue(signedOutStatus);

    await expect(call()).resolves.toEqual(signedOutStatus);
    expect(tauriMocks.invoke).toHaveBeenCalledOnce();
    expect(tauriMocks.invoke).toHaveBeenCalledWith(command);
  });

  it("forwards only normalized account status events and exposes cleanup", async () => {
    const unlisten = vi.fn();
    const listener = vi.fn();
    const signedInStatus: CodexRuntimeAuthStatus = {
      state: "signedIn",
      email: null,
      planType: "plus",
    };
    tauriMocks.listen.mockImplementation(
      async (
        eventName: string,
        handler: (event: { payload: CodexRuntimeAuthStatus }) => void,
      ) => {
        expect(eventName).toBe("account-status-changed");
        handler({ payload: signedInStatus });
        return unlisten;
      },
    );

    await expect(onAccountStatusChanged(listener)).resolves.toBe(unlisten);
    expect(tauriMocks.listen).toHaveBeenCalledOnce();
    expect(listener).toHaveBeenCalledOnce();
    expect(listener).toHaveBeenCalledWith(signedInStatus);
  });
});
