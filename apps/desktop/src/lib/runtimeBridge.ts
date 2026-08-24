import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { RuntimeStatus } from "../types/runtime";

const RUNTIME_STATUS_CHANGED_EVENT = "runtime-status-changed";

export function getRuntimeStatus(): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>("get_runtime_status");
}

export function retryAppServer(): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>("retry_app_server");
}

export function onRuntimeStatusChanged(
  listener: (status: RuntimeStatus) => void,
): Promise<UnlistenFn> {
  return listen<RuntimeStatus>(RUNTIME_STATUS_CHANGED_EVENT, (event) => {
    listener(event.payload);
  });
}
