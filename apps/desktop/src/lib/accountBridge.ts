import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { AccountStatus } from "../types/account";

const ACCOUNT_STATUS_CHANGED_EVENT = "account-status-changed";

export function getAccountStatus(): Promise<AccountStatus> {
  return invoke<AccountStatus>("get_account_status");
}

export function startChatgptLogin(): Promise<AccountStatus> {
  return invoke<AccountStatus>("start_chatgpt_login");
}

export function cancelAccountLogin(): Promise<AccountStatus> {
  return invoke<AccountStatus>("cancel_account_login");
}

export function logoutAccount(): Promise<AccountStatus> {
  return invoke<AccountStatus>("logout_account");
}

export function onAccountStatusChanged(
  listener: (status: AccountStatus) => void,
): Promise<UnlistenFn> {
  return listen<AccountStatus>(ACCOUNT_STATUS_CHANGED_EVENT, (event) => {
    listener(event.payload);
  });
}
