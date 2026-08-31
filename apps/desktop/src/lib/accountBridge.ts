import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { CodexRuntimeAuthStatus } from "../types/account";

const ACCOUNT_STATUS_CHANGED_EVENT = "account-status-changed";

export function getAccountStatus(): Promise<CodexRuntimeAuthStatus> {
  return invoke<CodexRuntimeAuthStatus>("get_account_status");
}

export function startChatgptLogin(): Promise<CodexRuntimeAuthStatus> {
  return invoke<CodexRuntimeAuthStatus>("start_chatgpt_login");
}

export function cancelAccountLogin(): Promise<CodexRuntimeAuthStatus> {
  return invoke<CodexRuntimeAuthStatus>("cancel_account_login");
}

export function logoutAccount(): Promise<CodexRuntimeAuthStatus> {
  return invoke<CodexRuntimeAuthStatus>("logout_account");
}

export function onAccountStatusChanged(
  listener: (status: CodexRuntimeAuthStatus) => void,
): Promise<UnlistenFn> {
  return listen<CodexRuntimeAuthStatus>(
    ACCOUNT_STATUS_CHANGED_EVENT,
    (event) => {
      listener(event.payload);
    },
  );
}
