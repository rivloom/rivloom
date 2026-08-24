import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { RuntimeStatus } from "../types/runtime";

const bridgeMocks = vi.hoisted(() => ({
  getRuntimeStatus: vi.fn(),
  onRuntimeStatusChanged: vi.fn(),
  retryAppServer: vi.fn(),
}));

vi.mock("../lib/runtimeBridge", () => bridgeMocks);

import { useRuntimeStatus } from "./useRuntimeStatus";

const connectedStatus: RuntimeStatus = {
  state: "connected",
  appVersion: "0.1.0-alpha.0",
  appServerUserAgent: "codex-app-server/1.2.3",
  platform: "windows/windows",
  codexHome: "C:\\Rivloom\\codex-home",
};

const errorStatus: RuntimeStatus = {
  state: "error",
  message: "核心服务暂时无法启动。",
  retryable: true,
};

describe("useRuntimeStatus", () => {
  beforeEach(() => {
    bridgeMocks.getRuntimeStatus.mockReset();
    bridgeMocks.onRuntimeStatusChanged.mockReset();
    bridgeMocks.retryAppServer.mockReset();
    bridgeMocks.getRuntimeStatus.mockResolvedValue({ state: "starting" });
    bridgeMocks.onRuntimeStatusChanged.mockResolvedValue(vi.fn());
  });

  it("reads the initial status exactly once", async () => {
    bridgeMocks.getRuntimeStatus.mockResolvedValue(connectedStatus);

    const { result } = renderHook(() => useRuntimeStatus());

    await waitFor(() => expect(result.current.status).toEqual(connectedStatus));
    expect(bridgeMocks.getRuntimeStatus).toHaveBeenCalledOnce();
    expect(bridgeMocks.onRuntimeStatusChanged).toHaveBeenCalledOnce();
  });

  it("applies later status events", async () => {
    let emitStatus: ((status: RuntimeStatus) => void) | undefined;
    bridgeMocks.onRuntimeStatusChanged.mockImplementation(async (listener) => {
      emitStatus = listener;
      return vi.fn();
    });

    const { result } = renderHook(() => useRuntimeStatus());
    await waitFor(() => expect(emitStatus).toBeDefined());

    act(() => emitStatus?.(connectedStatus));

    expect(result.current.status).toEqual(connectedStatus);
  });

  it("cleans up the status listener on unmount", async () => {
    const unlisten = vi.fn();
    bridgeMocks.onRuntimeStatusChanged.mockResolvedValue(unlisten);

    const { unmount } = renderHook(() => useRuntimeStatus());
    await waitFor(() =>
      expect(bridgeMocks.onRuntimeStatusChanged).toHaveBeenCalledOnce(),
    );

    unmount();

    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("allows only one retry while a retry is already pending", async () => {
    let finishRetry: ((status: RuntimeStatus) => void) | undefined;
    bridgeMocks.getRuntimeStatus.mockResolvedValue(errorStatus);
    bridgeMocks.retryAppServer.mockImplementation(
      () =>
        new Promise<RuntimeStatus>((resolve) => {
          finishRetry = resolve;
        }),
    );

    const { result } = renderHook(() => useRuntimeStatus());
    await waitFor(() => expect(result.current.status).toEqual(errorStatus));

    act(() => {
      void result.current.retry();
      void result.current.retry();
    });

    expect(bridgeMocks.retryAppServer).toHaveBeenCalledOnce();
    expect(result.current.retrying).toBe(true);

    await act(async () => finishRetry?.(connectedStatus));

    expect(result.current.status).toEqual(connectedStatus);
    expect(result.current.retrying).toBe(false);
  });
});
