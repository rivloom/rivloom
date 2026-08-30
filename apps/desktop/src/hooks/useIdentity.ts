import { useCallback, useEffect, useRef, useState } from "react";

import { getIdentity } from "../lib/identityBridge";
import type { RivloomIdentity } from "../types/identity";

export type IdentityState =
  | { state: "loading" }
  | { state: "ready"; identity: RivloomIdentity }
  | { state: "error"; message: string };

export type IdentityAction = "refresh";

const readErrorMessage = "Rivloom 身份暂时不可用。";

export function useIdentity() {
  const [state, setState] = useState<IdentityState>({ state: "loading" });
  const [pendingAction, setPendingAction] = useState<IdentityAction | null>(
    null,
  );
  const mountedRef = useRef(true);
  const refreshingRef = useRef(false);

  useEffect(() => {
    let active = true;
    mountedRef.current = true;
    void getIdentity()
      .then((identity) => {
        if (active) setState({ state: "ready", identity });
      })
      .catch(() => {
        if (active) setState({ state: "error", message: readErrorMessage });
      });
    return () => {
      active = false;
      mountedRef.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    if (refreshingRef.current) return;
    refreshingRef.current = true;
    setPendingAction("refresh");
    try {
      const identity = await getIdentity();
      if (mountedRef.current) setState({ state: "ready", identity });
    } catch {
      if (mountedRef.current) {
        setState({ state: "error", message: readErrorMessage });
      }
    } finally {
      refreshingRef.current = false;
      if (mountedRef.current) setPendingAction(null);
    }
  }, []);

  return { pendingAction, refresh, state };
}
