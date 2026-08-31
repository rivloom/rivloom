import { useCallback, useEffect, useRef, useState } from "react";
import {
  collaborationBridge,
  collaborationError,
  type CollaborationBridge,
} from "../lib/collaborationBridge";
import type {
  CollaborationError,
  HostingStatus,
  JoinBrainParams,
  MemberDirectory,
  NodeStatus,
} from "../types/collaboration";

export type CollaborationSnapshot = {
  host: HostingStatus;
  node: NodeStatus;
  directory: MemberDirectory | null;
};
export type CollaborationAction =
  | "read"
  | "initialize"
  | "start"
  | "stop"
  | "join"
  | "owner"
  | "connect"
  | "refresh"
  | "disconnect"
  | "invite"
  | "cancel"
  | "revoke";

async function read(
  bridge: CollaborationBridge,
): Promise<CollaborationSnapshot> {
  const [host, node] = await Promise.all([bridge.host(), bridge.node()]);
  const directory = node.state === "connected" ? await bridge.members() : null;
  return { host, node, directory };
}

// Reads never connect. Mutations are serialized and never retried, including uncertain failures.
export function useCollaboration(
  enabled: boolean,
  bridge = collaborationBridge,
) {
  const [snapshot, setSnapshot] = useState<CollaborationSnapshot | null>(null);
  const [pending, setPending] = useState<CollaborationAction | null>(null);
  const [error, setError] = useState<CollaborationError | null>(null);
  const epoch = useRef(0);
  const reads = useRef(0);
  const lock = useRef(false);
  const ready = useRef(false);
  const active = useRef(false);

  useEffect(() => {
    const generation = ++epoch.current;
    const readVersion = ++reads.current;
    active.current = enabled;
    ready.current = false;
    setSnapshot(null);
    setError(null);
    if (enabled) {
      void read(bridge)
        .then((value) => {
          if (generation === epoch.current && readVersion === reads.current) {
            ready.current = true;
            setSnapshot(value);
          }
        })
        .catch((failure) => {
          if (generation === epoch.current && readVersion === reads.current)
            setError(collaborationError(failure));
        });
    }
    return () => {
      active.current = false;
      ready.current = false;
      ++epoch.current;
    };
  }, [enabled, bridge]);

  const execute = useCallback(
    async <T>(
      name: CollaborationAction,
      command: () => Promise<T>,
    ): Promise<T | null> => {
      if (
        !active.current ||
        lock.current ||
        (name !== "read" && !ready.current)
      )
        return null;
      lock.current = true;
      const generation = epoch.current;
      const readVersion = ++reads.current;
      setPending(name);
      setError(null);
      let value: T | null = null;
      let failure: CollaborationError | null = null;
      try {
        value = await command();
      } catch (cause) {
        failure = collaborationError(cause);
      }
      // Re-read local status after success or uncertainty. This never resends the mutation.
      try {
        const next = await read(bridge);
        if (generation === epoch.current && readVersion === reads.current) {
          ready.current = true;
          setSnapshot(next);
        }
      } catch (cause) {
        failure ??= collaborationError(cause);
        if (generation === epoch.current && readVersion === reads.current) {
          ready.current = false;
          setSnapshot(null);
        }
      } finally {
        lock.current = false;
        if (active.current) setPending(null);
        if (generation === epoch.current) {
          setError(failure);
        }
      }
      // In particular, discard invitation results after navigation/unmount or failed status reads.
      return generation === epoch.current && !failure ? value : null;
    },
    [bridge],
  );

  return {
    snapshot,
    pending,
    error,
    reload: () => execute("read", async () => undefined),
    initialize: (params: { address: string; serverName: string }) =>
      execute("initialize", () => bridge.initialize(params)),
    start: () => execute("start", bridge.start),
    stop: () => execute("stop", bridge.stop),
    join: (params: JoinBrainParams) =>
      execute("join", () => bridge.join(params)),
    owner: (fingerprint: string) =>
      execute("owner", () => bridge.owner(fingerprint)),
    connect: () => execute("connect", bridge.connect),
    refresh: () => execute("refresh", bridge.refresh),
    disconnect: () => execute("disconnect", bridge.disconnect),
    invite: () => execute("invite", bridge.invite),
    cancel: (invitationId: string) =>
      execute("cancel", () => bridge.cancel(invitationId)),
    revoke: (memberId: string) =>
      execute("revoke", () => bridge.revoke(memberId)),
  };
}

export type CollaborationController = ReturnType<typeof useCollaboration>;
