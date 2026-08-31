export type TrustDescriptor = {
  version: 1;
  brainId: string;
  address: string;
  serverName: string;
  certificateDer: number[];
};

export type CredentialBinding = {
  brainId: string;
  memberId: string;
  nodeId: string;
  deviceId: string;
};

export type HostProfile = {
  version: 1;
  binding: CredentialBinding;
  descriptor: TrustDescriptor;
  credentialExpiresAt: number;
};

export type HostingStatus =
  | { state: "notConfigured" | "faulted" }
  | { state: "stopped" | "running"; profile: HostProfile };

export type NodeRegistration = {
  version: 1;
  identityId: string;
  deviceId: string;
  descriptor: TrustDescriptor;
  confirmedFingerprint: string;
};

export type NodeStatus = {
  state: "notConfigured" | "recoveryRequired" | "disconnected" | "connected";
  registration: NodeRegistration | null;
  binding: CredentialBinding | null;
  revision: number;
};

export type MemberEntry = {
  type: "member";
  memberId: string;
  displayName: string;
  owner: boolean;
  revoked: boolean;
};

export type MemberDirectory = {
  revision: number;
  entries: (
    | MemberEntry
    | {
        type: "node";
        nodeId: string;
        memberId: string;
        online: boolean;
        lastSeenAt: number | null;
      }
  )[];
};

// Transient user transfer only. Never put invitations in persisted app state or diagnostics.
export type BrainInvitation = {
  brainId: string;
  invitationId: string;
  expiresAt: number;
  secret: string;
};

export type JoinBrainParams = {
  descriptor: string;
  confirmedFingerprint: string;
  invitation: BrainInvitation;
};

export type CollaborationError =
  | "invalid"
  | "notConfigured"
  | "incomplete"
  | "recoveryRequired"
  | "existing"
  | "storage"
  | "busy"
  | "disconnected"
  | "transport"
  | "credential"
  | "rejected"
  | "unavailable";
