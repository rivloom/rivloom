export type BrainMembershipRole = "owner" | "member";

export type BrainMembershipSummary = {
  brainId: string;
  memberId: string;
  role: BrainMembershipRole;
};

export type RivloomIdentity = {
  identityId: string;
  displayName: string;
  deviceId: string;
  brainMembership: BrainMembershipSummary | null;
};
