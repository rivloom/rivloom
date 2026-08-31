use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::credential::{
    AccessError, Clock, ConnectionIdentity, CredentialBinding, CredentialRegistry,
    IssuedCredential, SecretToken,
};
use super::invitation::{Enrollment, InvitationRegistry, IssuedInvitation, JoinRequest};
use super::protocol::{MAX_REVISION, id, timestamp};

pub(super) const MAX_BRAIN_BYTES: usize = 2 * 1024 * 1024;
const PRESENCE_TTL_SECONDS: i64 = 30;

pub(super) struct OwnerProfile<'a> {
    pub(super) identity_id: &'a str,
    pub(super) device_id: &'a str,
    pub(super) display_name: &'a str,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MemberRecord {
    pub(super) identity_id: String,
    pub(super) display_name: String,
    pub(super) revoked: bool,
    pub(super) revision: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct NodeRecord {
    pub(super) member_id: String,
    pub(super) device_id: String,
    pub(super) last_seen_at: Option<i64>,
    pub(super) online: bool,
    pub(super) revision: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BrainSnapshot {
    version: u32,
    pub(super) brain_id: String,
    pub(super) owner_member_id: String,
    pub(super) revision: u64,
    pub(super) clock: Clock,
    pub(super) credentials: CredentialRegistry,
    pub(super) invitations: InvitationRegistry,
    #[serde(deserialize_with = "super::snapshot::unique_map::<_, _, 64>")]
    pub(super) members: BTreeMap<String, MemberRecord>,
    #[serde(deserialize_with = "super::snapshot::unique_map::<_, _, 64>")]
    pub(super) nodes: BTreeMap<String, NodeRecord>,
}

/// In-memory authority; durable callers must commit the entire mutation before returning its result.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub(super) struct Brain {
    pub(super) state: BrainSnapshot,
}

pub(super) enum PresenceReset {
    Expired,
    Restart,
}

impl Brain {
    /// Trusted local owner initialization, never a remote self-registration endpoint.
    pub(super) fn bootstrap(
        brain_id: String,
        owner: OwnerProfile<'_>,
        now: i64,
    ) -> Result<(Self, IssuedCredential), BrainError> {
        if !id(owner.identity_id) || !id(owner.device_id) || !display_name(owner.display_name) {
            return Err(BrainError::Invalid);
        }
        let mut clock = Clock::default();
        clock.observe(now)?;
        let mut credentials = CredentialRegistry::new(brain_id.clone())?;
        let invitations = InvitationRegistry::new(brain_id.clone())?;
        let owner_member_id = format!("member-{}", SecretToken::generate()?.expose_secret());
        let credential = credentials.issue(
            CredentialBinding {
                brain_id: brain_id.clone(),
                member_id: owner_member_id.clone(),
                node_id: format!("node-{}", SecretToken::generate()?.expose_secret()),
                device_id: owner.device_id.into(),
            },
            now,
        )?;
        let members = BTreeMap::from([(
            owner_member_id.clone(),
            MemberRecord {
                identity_id: owner.identity_id.into(),
                display_name: owner.display_name.into(),
                revoked: false,
                revision: 1,
            },
        )]);
        let nodes = BTreeMap::from([(
            credential.binding.node_id.clone(),
            NodeRecord {
                member_id: owner_member_id.clone(),
                device_id: owner.device_id.into(),
                last_seen_at: None,
                online: false,
                revision: 1,
            },
        )]);
        Ok((
            Self {
                state: BrainSnapshot {
                    version: 1,
                    brain_id,
                    owner_member_id,
                    revision: 1,
                    clock,
                    credentials,
                    invitations,
                    members,
                    nodes,
                },
            },
            credential,
        ))
    }

    pub(super) fn brain_id(&self) -> &str {
        &self.state.brain_id
    }
    pub(super) fn revision(&self) -> u64 {
        self.state.revision
    }

    pub(super) fn observe(&mut self, now: i64) -> Result<(), BrainError> {
        self.state.clock.observe(now).map_err(Into::into)
    }

    pub(super) fn connect(
        &mut self,
        binding: &CredentialBinding,
        secret: &SecretToken,
        now: i64,
    ) -> Result<ConnectionIdentity, BrainError> {
        self.observe(now)?;
        self.state
            .credentials
            .connect(binding, secret, now)
            .map_err(Into::into)
    }

    pub(super) fn create_invitation(&mut self, now: i64) -> Result<IssuedInvitation, BrainError> {
        self.observe(now)?;
        let revision = self.next_revision()?;
        let invitation = self.state.invitations.create(now)?;
        self.state.revision = revision;
        Ok(invitation)
    }

    pub(super) fn cancel_invitation(
        &mut self,
        invitation_id: &str,
        now: i64,
    ) -> Result<(), BrainError> {
        self.observe(now)?;
        let revision = self.next_revision()?;
        self.state.invitations.cancel(invitation_id)?;
        self.state.revision = revision;
        Ok(())
    }

    pub(super) fn join(
        &mut self,
        request: JoinRequest<'_>,
        now: i64,
    ) -> Result<Enrollment, BrainError> {
        self.observe(now)?;
        if !display_name(request.display_name) {
            return Err(BrainError::Invalid);
        }
        let revision = self.next_revision()?;
        let enrollment =
            self.state
                .invitations
                .redeem(request, &mut self.state.credentials, now)?;
        let binding = &enrollment.credential.binding;
        if self.state.members.contains_key(&binding.member_id) {
            return Err(BrainError::Conflict);
        }
        self.state.members.insert(
            binding.member_id.clone(),
            MemberRecord {
                identity_id: enrollment.member.identity_id.clone(),
                display_name: enrollment.member.display_name.clone(),
                revoked: false,
                revision,
            },
        );
        self.state.nodes.insert(
            binding.node_id.clone(),
            NodeRecord {
                member_id: binding.member_id.clone(),
                device_id: binding.device_id.clone(),
                last_seen_at: None,
                online: false,
                revision,
            },
        );
        self.state.revision = revision;
        Ok(enrollment)
    }

    /// Trusted local administration; owner transfer/revocation is deliberately not exposed.
    pub(super) fn revoke_member(
        &mut self,
        member_id: &str,
        expected_revision: u64,
        now: i64,
    ) -> Result<u64, BrainError> {
        self.observe(now)?;
        self.check_revision(expected_revision)?;
        let member = self
            .state
            .members
            .get(member_id)
            .ok_or(BrainError::Invalid)?;
        if member_id == self.state.owner_member_id {
            return Err(BrainError::Invalid);
        }
        if member.revoked {
            return Ok(self.revision());
        }
        let revision = self.next_revision()?;
        self.state.credentials.revoke_member(member_id)?;
        let member = self
            .state
            .members
            .get_mut(member_id)
            .ok_or(BrainError::Invalid)?;
        member.revoked = true;
        member.revision = revision;
        for node in self
            .state
            .nodes
            .values_mut()
            .filter(|node| node.member_id == member_id)
        {
            node.online = false;
            node.revision = revision;
        }
        self.state.revision = revision;
        Ok(revision)
    }

    /// Internal heartbeat mutation. R3.3 replay admission wraps this before any network use.
    pub(super) fn heartbeat(
        &mut self,
        session: &ConnectionIdentity,
        expected_revision: u64,
        now: i64,
    ) -> Result<u64, BrainError> {
        self.observe(now)?;
        self.state.credentials.authorize_task(session, now)?;
        self.check_revision(expected_revision)?;
        let revision = self.next_revision()?;
        let node = self
            .state
            .nodes
            .get_mut(&session.binding().node_id)
            .ok_or(BrainError::Invalid)?;
        node.last_seen_at = Some(now);
        node.online = true;
        node.revision = revision;
        self.state.revision = revision;
        Ok(revision)
    }

    pub(super) fn reset_presence(
        &mut self,
        now: i64,
        reason: PresenceReset,
    ) -> Result<u64, BrainError> {
        self.observe(now)?;
        let expired = |node: &NodeRecord| {
            node.online
                && match reason {
                    PresenceReset::Restart => true,
                    PresenceReset::Expired => node
                        .last_seen_at
                        .is_none_or(|seen| now.saturating_sub(seen) >= PRESENCE_TTL_SECONDS),
                }
        };
        if self.state.nodes.values().any(expired) {
            let revision = self.next_revision()?;
            for node in self.state.nodes.values_mut().filter(|node| expired(node)) {
                node.online = false;
                node.revision = revision;
            }
            self.state.revision = revision;
        }
        Ok(self.revision())
    }

    pub(super) fn check_revision(&self, expected: u64) -> Result<(), BrainError> {
        if expected != self.revision() {
            return Err(BrainError::Conflict);
        }
        Ok(())
    }

    pub(super) fn next_revision(&self) -> Result<u64, BrainError> {
        self.revision()
            .checked_add(1)
            .filter(|v| *v <= MAX_REVISION)
            .ok_or(BrainError::Capacity)
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, BrainError> {
        self.validate()?;
        let bytes = serde_json::to_vec(&self.state).map_err(|_| BrainError::Invalid)?;
        if bytes.len() > MAX_BRAIN_BYTES {
            return Err(BrainError::Capacity);
        }
        Ok(bytes)
    }

    /// Only the bounded snapshot boundary may reconstruct authority; errors never create an empty Brain.
    pub(super) fn decode(bytes: &[u8], brain_id: &str) -> Result<Self, BrainError> {
        if bytes.len() > MAX_BRAIN_BYTES {
            return Err(BrainError::Capacity);
        }
        let state = serde_json::from_slice(bytes).map_err(|_| BrainError::Invalid)?;
        let brain = Self { state };
        brain.validate()?;
        if brain.brain_id() != brain_id {
            return Err(BrainError::Invalid);
        }
        Ok(brain)
    }

    fn validate(&self) -> Result<(), BrainError> {
        let s = &self.state;
        if s.version != 1
            || !id(&s.brain_id)
            || s.revision == 0
            || s.revision > MAX_REVISION
            || !timestamp(s.clock.high_water_at())
            || s.credentials.brain_id() != s.brain_id
            || s.invitations.brain_id() != s.brain_id
            || s.credentials.high_water_at() > s.clock.high_water_at()
            || s.invitations.high_water_at() > s.clock.high_water_at()
            || s.members.is_empty()
            || s.members.len() > 64
            || s.nodes.len() > 64
            || !s
                .members
                .get(&s.owner_member_id)
                .is_some_and(|member| !member.revoked)
            || s.credentials.bindings().count() != s.nodes.len()
        {
            return Err(BrainError::Invalid);
        }
        for (key, member) in &s.members {
            if !id(key)
                || !id(&member.identity_id)
                || !display_name(&member.display_name)
                || member.revision == 0
                || member.revision > s.revision
                || !s
                    .credentials
                    .bindings()
                    .any(|binding| &binding.member_id == key)
                || member.revoked != s.credentials.is_revoked(key)
            {
                return Err(BrainError::Invalid);
            }
        }
        for binding in s.credentials.bindings() {
            let node = s.nodes.get(&binding.node_id).ok_or(BrainError::Invalid)?;
            let member = s
                .members
                .get(&binding.member_id)
                .ok_or(BrainError::Invalid)?;
            if node.member_id != binding.member_id
                || node.device_id != binding.device_id
                || node.revision == 0
                || node.revision > s.revision
                || (member.revoked && node.online)
                || (node.online && node.last_seen_at.is_none())
                || node
                    .last_seen_at
                    .is_some_and(|seen| !timestamp(seen) || seen > s.clock.high_water_at())
            {
                return Err(BrainError::Invalid);
            }
        }
        Ok(())
    }
}

fn display_name(name: &str) -> bool {
    name.len() <= 80 && !name.trim().is_empty() && !name.chars().any(char::is_control)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum BrainError {
    #[error("Invalid collaboration state")]
    Invalid,
    #[error("Collaboration revision or replay conflict")]
    Conflict,
    #[error("Collaboration state capacity reached")]
    Capacity,
    #[error(transparent)]
    Access(#[from] AccessError),
}

#[cfg(test)]
#[path = "brain_tests.rs"]
mod tests;
