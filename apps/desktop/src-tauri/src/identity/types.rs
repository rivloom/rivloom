use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RivloomIdentity {
    pub(crate) identity_id: String,
    pub(crate) display_name: String,
    pub(crate) device_id: String,
    pub(crate) brain_membership: Option<BrainMembershipSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrainMembershipSummary {
    pub(crate) brain_id: String,
    pub(crate) member_id: String,
    pub(crate) role: BrainMembershipRole,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BrainMembershipRole {
    Owner,
    Member,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
