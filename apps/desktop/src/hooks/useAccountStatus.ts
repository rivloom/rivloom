import { useCallback, useEffect, useRef, useState } from "react";

import {
  cancelAccountLogin,
  getAccountStatus,
  logoutAccount,
  onAccountStatusChanged,
  startChatgptLogin,
} from "../lib/accountBridge";
import type { AccountStatus } from "../types/account";

export type AccountAction =
  | "refresh"
  | "startChatgptLogin"
  | "cancelLogin"
  | "logout";

const checkingStatus: AccountStatus = { state: "checking" };
const unavailableStatus: AccountStatus = {
  state: "error",
  message: "账号状态暂时不可用。",
  retryable: true,
};

export function useAccountStatus(runtimeConnected: boolean) {
  const [status, setStatus] = useState<AccountStatus>(checkingStatus);
  const [pendingAction, setPendingAction] = useState<AccountAction | null>(
    null,
  );
  const actionTokenRef = useRef<symbol | null>(null);
  const connectedRef = useRef(runtimeConnected);
  const eventRevisionRef = useRef(0);
  const lifecycleRef = useRef(0);
  const mountedRef = useRef(true);

  connectedRef.current = runtimeConnected;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    const lifecycle = ++lifecycleRef.current;
    let active = true;
    let unlisten: (() => void) | undefined;
    eventRevisionRef.current = 0;
    actionTokenRef.current = null;
    setPendingAction(null);
    setStatus(checkingStatus);

    if (!runtimeConnected) {
      return () => {
        active = false;
      };
    }

    void (async () => {
      try {
        unlisten = await onAccountStatusChanged((nextStatus) => {
          if (!active || lifecycleRef.current !== lifecycle) {
            return;
          }

          eventRevisionRef.current += 1;
          setStatus(nextStatus);
        });
      } catch {
        if (active && lifecycleRef.current === lifecycle) {
          setStatus(unavailableStatus);
        }
        return;
      }

      if (!active || lifecycleRef.current !== lifecycle) {
        unlisten();
        return;
      }

      const revisionBeforeRead = eventRevisionRef.current;
      try {
        const currentStatus = await getAccountStatus();
        if (
          active &&
          lifecycleRef.current === lifecycle &&
          eventRevisionRef.current === revisionBeforeRead
        ) {
          setStatus(currentStatus);
        }
      } catch {
        if (
          active &&
          lifecycleRef.current === lifecycle &&
          eventRevisionRef.current === revisionBeforeRead
        ) {
          setStatus(unavailableStatus);
        }
      }
    })();

    return () => {
      active = false;
      unlisten?.();
    };
  }, [runtimeConnected]);

  const runAction = useCallback(
    async (
      action: AccountAction,
      call: () => Promise<AccountStatus>,
    ): Promise<void> => {
      if (!connectedRef.current || actionTokenRef.current !== null) {
        return;
      }

      const token = Symbol(action);
      const lifecycle = lifecycleRef.current;
      const revisionBeforeCall = eventRevisionRef.current;
      actionTokenRef.current = token;
      setPendingAction(action);

      try {
        const nextStatus = await call();
        if (
          mountedRef.current &&
          connectedRef.current &&
          lifecycleRef.current === lifecycle &&
          eventRevisionRef.current === revisionBeforeCall
        ) {
          setStatus(nextStatus);
        }
      } catch {
        if (
          mountedRef.current &&
          connectedRef.current &&
          lifecycleRef.current === lifecycle &&
          eventRevisionRef.current === revisionBeforeCall
        ) {
          setStatus(unavailableStatus);
        }
      } finally {
        if (actionTokenRef.current === token) {
          actionTokenRef.current = null;
          if (mountedRef.current) {
            setPendingAction(null);
          }
        }
      }
    },
    [],
  );

  const refresh = useCallback(
    () => runAction("refresh", getAccountStatus),
    [runAction],
  );
  const beginChatgptLogin = useCallback(
    () => runAction("startChatgptLogin", startChatgptLogin),
    [runAction],
  );
  const cancelLogin = useCallback(
    () => runAction("cancelLogin", cancelAccountLogin),
    [runAction],
  );
  const logout = useCallback(
    () => runAction("logout", logoutAccount),
    [runAction],
  );
  return {
    beginChatgptLogin,
    cancelLogin,
    logout,
    pendingAction,
    refresh,
    status,
  };
}
