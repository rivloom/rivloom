import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { CodexRuntimeAuthStatus } from "../types/account";

const bridgeMocks = vi.hoisted(() => ({
  cancelAccountLogin: vi.fn(),
  getAccountStatus: vi.fn(),
  logoutAccount: vi.fn(),
  onAccountStatusChanged: vi.fn(),
  startChatgptLogin: vi.fn(),
}));

vi.mock("../lib/accountBridge", () => bridgeMocks);

import { useAccountStatus } from "./useAccountStatus";

const checkingStatus: CodexRuntimeAuthStatus = { state: "checking" };
const signedOutStatus: CodexRuntimeAuthStatus = { state: "signedOut" };
const browserPendingStatus: CodexRuntimeAuthStatus = {
  state: "browserPending",
};
const signedInStatus: CodexRuntimeAuthStatus = {
  state: "signedIn",
  email: "user@example.com",
  planType: "plus",
};
const unavailableStatus: CodexRuntimeAuthStatus = {
  state: "error",
  message: "账号状态暂时不可用。",
  retryable: true,
};

describe("useAccountStatus", () => {
  beforeEach(() => {
    bridgeMocks.cancelAccountLogin.mockReset();
    bridgeMocks.getAccountStatus.mockReset();
    bridgeMocks.logoutAccount.mockReset();
    bridgeMocks.onAccountStatusChanged.mockReset();
    bridgeMocks.startChatgptLogin.mockReset();

    bridgeMocks.getAccountStatus.mockResolvedValue(signedOutStatus);
    bridgeMocks.onAccountStatusChanged.mockResolvedValue(vi.fn());
    bridgeMocks.cancelAccountLogin.mockResolvedValue(signedOutStatus);
    bridgeMocks.logoutAccount.mockResolvedValue(signedOutStatus);
    bridgeMocks.startChatgptLogin.mockResolvedValue(browserPendingStatus);
  });

  it("waits for the runtime and subscribes before the initial read", async () => {
    const callOrder: string[] = [];
    bridgeMocks.onAccountStatusChanged.mockImplementation(async () => {
      callOrder.push("subscribe");
      return vi.fn();
    });
    bridgeMocks.getAccountStatus.mockImplementation(async () => {
      callOrder.push("read");
      return signedOutStatus;
    });

    const { rerender, result } = renderHook(
      ({ connected }) => useAccountStatus(connected),
      { initialProps: { connected: false } },
    );

    await act(async () => undefined);
    expect(result.current.status).toEqual(checkingStatus);
    expect(bridgeMocks.onAccountStatusChanged).not.toHaveBeenCalled();
    expect(bridgeMocks.getAccountStatus).not.toHaveBeenCalled();

    rerender({ connected: true });

    await waitFor(() => expect(result.current.status).toEqual(signedOutStatus));
    expect(callOrder).toEqual(["subscribe", "read"]);
  });

  it("does not let a stale initial read replace a newer event", async () => {
    let emitStatus: ((status: CodexRuntimeAuthStatus) => void) | undefined;
    let finishRead: ((status: CodexRuntimeAuthStatus) => void) | undefined;
    bridgeMocks.onAccountStatusChanged.mockImplementation(async (listener) => {
      emitStatus = listener;
      return vi.fn();
    });
    bridgeMocks.getAccountStatus.mockImplementation(
      () =>
        new Promise<CodexRuntimeAuthStatus>((resolve) => {
          finishRead = resolve;
        }),
    );

    const { result } = renderHook(() => useAccountStatus(true));
    await waitFor(() => {
      expect(emitStatus).toBeDefined();
      expect(finishRead).toBeDefined();
    });

    act(() => emitStatus?.(signedInStatus));
    expect(result.current.status).toEqual(signedInStatus);

    await act(async () => finishRead?.(signedOutStatus));
    expect(result.current.status).toEqual(signedInStatus);
  });

  it("does not let a rejected stale read replace a newer event", async () => {
    let emitStatus: ((status: CodexRuntimeAuthStatus) => void) | undefined;
    let failRead: ((error: Error) => void) | undefined;
    bridgeMocks.onAccountStatusChanged.mockImplementation(async (listener) => {
      emitStatus = listener;
      return vi.fn();
    });
    bridgeMocks.getAccountStatus.mockImplementation(
      () =>
        new Promise<CodexRuntimeAuthStatus>((_resolve, reject) => {
          failRead = reject;
        }),
    );

    const { result } = renderHook(() => useAccountStatus(true));
    await waitFor(() => {
      expect(emitStatus).toBeDefined();
      expect(failRead).toBeDefined();
    });

    act(() => emitStatus?.(signedInStatus));
    await act(async () => failRead?.(new Error("stale backend failure")));

    expect(result.current.status).toEqual(signedInStatus);
  });

  it("cleans the old listener and rereads after reconnecting", async () => {
    const firstUnlisten = vi.fn();
    const secondUnlisten = vi.fn();
    bridgeMocks.onAccountStatusChanged
      .mockResolvedValueOnce(firstUnlisten)
      .mockResolvedValueOnce(secondUnlisten);
    bridgeMocks.getAccountStatus
      .mockResolvedValueOnce(signedOutStatus)
      .mockResolvedValueOnce(signedInStatus);

    const { rerender, result, unmount } = renderHook(
      ({ connected }) => useAccountStatus(connected),
      { initialProps: { connected: true } },
    );
    await waitFor(() => expect(result.current.status).toEqual(signedOutStatus));

    rerender({ connected: false });
    await waitFor(() => expect(firstUnlisten).toHaveBeenCalledOnce());
    expect(result.current.status).toEqual(checkingStatus);

    rerender({ connected: true });
    await waitFor(() => expect(result.current.status).toEqual(signedInStatus));
    expect(bridgeMocks.getAccountStatus).toHaveBeenCalledTimes(2);
    expect(bridgeMocks.onAccountStatusChanged).toHaveBeenCalledTimes(2);

    unmount();
    expect(secondUnlisten).toHaveBeenCalledOnce();
  });

  it("ignores events from a listener after that connection is gone", async () => {
    let emitOldStatus: ((status: CodexRuntimeAuthStatus) => void) | undefined;
    bridgeMocks.onAccountStatusChanged
      .mockImplementationOnce(async (listener) => {
        emitOldStatus = listener;
        return vi.fn();
      })
      .mockResolvedValueOnce(vi.fn());
    bridgeMocks.getAccountStatus
      .mockResolvedValueOnce(signedOutStatus)
      .mockResolvedValueOnce(signedInStatus);

    const { rerender, result } = renderHook(
      ({ connected }) => useAccountStatus(connected),
      { initialProps: { connected: true } },
    );
    await waitFor(() => expect(result.current.status).toEqual(signedOutStatus));

    rerender({ connected: false });
    rerender({ connected: true });
    await waitFor(() => expect(result.current.status).toEqual(signedInStatus));

    act(() => emitOldStatus?.(browserPendingStatus));
    expect(result.current.status).toEqual(signedInStatus);
  });

  it("deduplicates account actions while one is pending", async () => {
    let finishLogin: ((status: CodexRuntimeAuthStatus) => void) | undefined;
    bridgeMocks.startChatgptLogin.mockImplementation(
      () =>
        new Promise<CodexRuntimeAuthStatus>((resolve) => {
          finishLogin = resolve;
        }),
    );

    const { result } = renderHook(() => useAccountStatus(true));
    await waitFor(() => expect(result.current.status).toEqual(signedOutStatus));

    act(() => {
      void result.current.beginChatgptLogin();
      void result.current.beginChatgptLogin();
      void result.current.logout();
    });

    expect(bridgeMocks.startChatgptLogin).toHaveBeenCalledOnce();
    expect(bridgeMocks.logoutAccount).not.toHaveBeenCalled();
    expect(result.current.pendingAction).toBe("startChatgptLogin");

    await act(async () => finishLogin?.(browserPendingStatus));
    expect(result.current.status).toEqual(browserPendingStatus);
    expect(result.current.pendingAction).toBeNull();
  });

  it("maps a rejected initial read to the safe account error", async () => {
    bridgeMocks.getAccountStatus.mockRejectedValue(
      new Error("private backend detail"),
    );

    const { result } = renderHook(() => useAccountStatus(true));

    await waitFor(() =>
      expect(result.current.status).toEqual(unavailableStatus),
    );
  });

  it("maps a rejected action to the safe account error", async () => {
    bridgeMocks.startChatgptLogin.mockRejectedValue(
      new Error("private backend detail"),
    );

    const { result } = renderHook(() => useAccountStatus(true));
    await waitFor(() => expect(result.current.status).toEqual(signedOutStatus));

    await act(async () => result.current.beginChatgptLogin());

    expect(result.current.status).toEqual(unavailableStatus);
    expect(result.current.pendingAction).toBeNull();
  });
});
