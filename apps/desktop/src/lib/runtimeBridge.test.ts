import { beforeEach, describe, expect, it, vi } from "vitest";

import type { RuntimeStatus } from "../types/runtime";

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
  getRuntimeStatus,
  onRuntimeStatusChanged,
  retryAppServer,
} from "./runtimeBridge";

const connectedStatus: RuntimeStatus = {
  state: "connected",
  appVersion: "0.1.0-alpha.0",
  appServerUserAgent: "codex-app-server/1.2.3",
  platform: "windows/windows",
  codexHome: "C:\\Rivloom\\codex-home",
};

describe("runtimeBridge", () => {
  beforeEach(() => {
    tauriMocks.invoke.mockReset();
    tauriMocks.listen.mockReset();
  });

  it("reads the initial status with only the fixed Rivloom command", async () => {
    tauriMocks.invoke.mockResolvedValue(connectedStatus);

    await expect(getRuntimeStatus()).resolves.toEqual(connectedStatus);
    expect(tauriMocks.invoke).toHaveBeenCalledOnce();
    expect(tauriMocks.invoke).toHaveBeenCalledWith("get_runtime_status");
  });

  it("retries with only the fixed Rivloom command", async () => {
    tauriMocks.invoke.mockResolvedValue(connectedStatus);

    await expect(retryAppServer()).resolves.toEqual(connectedStatus);
    expect(tauriMocks.invoke).toHaveBeenCalledOnce();
    expect(tauriMocks.invoke).toHaveBeenCalledWith("retry_app_server");
  });

  it("forwards typed status events and returns Tauri's cleanup function", async () => {
    const unlisten = vi.fn();
    const listener = vi.fn();
    tauriMocks.listen.mockImplementation(
      async (
        eventName: string,
        handler: (event: { payload: RuntimeStatus }) => void,
      ) => {
        expect(eventName).toBe("runtime-status-changed");
        handler({ payload: connectedStatus });
        return unlisten;
      },
    );

    await expect(onRuntimeStatusChanged(listener)).resolves.toBe(unlisten);
    expect(listener).toHaveBeenCalledOnce();
    expect(listener).toHaveBeenCalledWith(connectedStatus);
  });
});
