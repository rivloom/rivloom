import { useCallback, useEffect, useRef, useState } from "react";

import {
  getRuntimeStatus,
  onRuntimeStatusChanged,
  retryAppServer,
} from "../lib/runtimeBridge";
import type { RuntimeStatus } from "../types/runtime";

const initialStatus: RuntimeStatus = { state: "starting" };
const unavailableStatus: RuntimeStatus = {
  state: "error",
  message: "核心服务暂时无法连接。",
  retryable: true,
};

export function useRuntimeStatus() {
  const [status, setStatus] = useState<RuntimeStatus>(initialStatus);
  const [retrying, setRetrying] = useState(false);
  const retryingRef = useRef(false);
  const mountedRef = useRef(true);
  const initialReadRef = useRef<Promise<RuntimeStatus> | null>(null);

  useEffect(() => {
    mountedRef.current = true;
    let active = true;
    let unlisten: (() => void) | undefined;
    let eventRevision = 0;

    void (async () => {
      try {
        unlisten = await onRuntimeStatusChanged((nextStatus) => {
          eventRevision += 1;
          if (active) {
            setStatus(nextStatus);
          }
        });

        if (!active) {
          unlisten();
          return;
        }

        const revisionBeforeRead = eventRevision;
        initialReadRef.current ??= getRuntimeStatus();
        const currentStatus = await initialReadRef.current;
        if (active && eventRevision === revisionBeforeRead) {
          setStatus(currentStatus);
        }
      } catch {
        if (active) {
          setStatus(unavailableStatus);
        }
      }
    })();

    return () => {
      active = false;
      mountedRef.current = false;
      unlisten?.();
    };
  }, []);

  const retry = useCallback(async () => {
    if (retryingRef.current) {
      return;
    }

    retryingRef.current = true;
    setRetrying(true);
    try {
      const nextStatus = await retryAppServer();
      if (mountedRef.current) {
        setStatus(nextStatus);
      }
    } catch {
      if (mountedRef.current) {
        setStatus(unavailableStatus);
      }
    } finally {
      retryingRef.current = false;
      if (mountedRef.current) {
        setRetrying(false);
      }
    }
  }, []);

  return { retry, retrying, status };
}
