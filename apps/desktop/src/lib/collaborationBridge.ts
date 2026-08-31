import { invoke } from "@tauri-apps/api/core";
import type {
  BrainInvitation,
  CollaborationError,
  HostProfile,
  HostingStatus,
  JoinBrainParams,
  MemberDirectory,
  NodeStatus,
} from "../types/collaboration";

// Explicit commands only: importing this module does not connect, enroll, listen, or retry.
export const collaborationBridge = {
  host: () => invoke<HostingStatus>("get_local_brain_status"),
  initialize: (params: { address: string; serverName: string }) =>
    invoke<HostProfile>("initialize_local_brain", { params }),
  start: () => invoke<HostingStatus>("start_local_brain"),
  stop: () => invoke<void>("stop_local_brain"),
  node: () => invoke<NodeStatus>("get_node_status"),
  join: (params: JoinBrainParams) =>
    invoke<NodeStatus>("join_brain", { params }),
  owner: (confirmedFingerprint: string) =>
    invoke<NodeStatus>("connect_brain_owner", {
      params: { confirmedFingerprint },
    }),
  connect: () => invoke<NodeStatus>("connect_brain"),
  refresh: () => invoke<NodeStatus>("refresh_brain"),
  disconnect: () => invoke<void>("disconnect_brain"),
  members: () => invoke<MemberDirectory>("list_brain_members"),
  invite: () => invoke<BrainInvitation>("create_brain_invitation"),
  cancel: (invitationId: string) =>
    invoke<void>("cancel_brain_invitation", { params: { invitationId } }),
  revoke: (memberId: string) =>
    invoke<void>("revoke_brain_member", { params: { memberId } }),
};

export type CollaborationBridge = typeof collaborationBridge;

export function collaborationError(error: unknown): CollaborationError {
  switch (error) {
    case "invalid":
    case "notConfigured":
    case "incomplete":
    case "recoveryRequired":
    case "existing":
    case "storage":
    case "busy":
    case "disconnected":
    case "transport":
    case "credential":
    case "rejected":
    case "unavailable":
      return error;
    default:
      return "unavailable";
  }
}
