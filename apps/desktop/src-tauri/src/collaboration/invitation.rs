use std::collections::BTreeMap;

use serde::Serialize;

use super::credential::{
    AccessError, Clock, CredentialBinding, CredentialRegistry, IssuedCredential, SecretPurpose,
    SecretToken,
};
use super::protocol::{id, timestamp};

const MAX_PENDING_INVITATIONS: usize = 32;
const INVITATION_TTL_SECONDS: i64 = 10 * 60;

#[derive(Debug)]
pub(super) struct IssuedInvitation {
    pub(super) brain_id: String,
    pub(super) invitation_id: String,
    pub(super) expires_at: i64,
    pub(super) secret: SecretToken,
}

/// All fields are untrusted; possession of the invitation does not prove a human/device identity.
pub(super) struct JoinRequest<'a> {
    pub(super) brain_id: &'a str,
    pub(super) invitation_id: &'a str,
    pub(super) secret: &'a SecretToken,
    pub(super) identity_id: &'a str,
    pub(super) device_id: &'a str,
    pub(super) display_name: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum JoinedRole {
    Member,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JoinedMember {
    pub(super) member_id: String,
    pub(super) identity_id: String,
    pub(super) display_name: String,
    pub(super) role: JoinedRole,
}

#[derive(Debug)]
pub(super) struct Enrollment {
    pub(super) member: JoinedMember,
    pub(super) credential: IssuedCredential,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingInvitation {
    verifier: [u8; 32],
    issued_at: i64,
    expires_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InvitationRegistry {
    brain_id: String,
    clock: Clock,
    pending: BTreeMap<String, PendingInvitation>,
}

impl InvitationRegistry {
    pub(super) fn new(brain_id: String) -> Result<Self, AccessError> {
        if !id(&brain_id) {
            return Err(AccessError::Rejected);
        }
        Ok(Self {
            brain_id,
            clock: Clock::default(),
            pending: BTreeMap::new(),
        })
    }

    /// Trusted Brain administration only; never expose directly to unauthenticated peers.
    pub(super) fn create(&mut self, now: i64) -> Result<IssuedInvitation, AccessError> {
        self.clock.observe(now)?;
        let expires_at = now
            .checked_add(INVITATION_TTL_SECONDS)
            .filter(|value| timestamp(*value))
            .ok_or(AccessError::Rejected)?;
        self.pending
            .retain(|_, invitation| now < invitation.expires_at);
        if self.pending.len() >= MAX_PENDING_INVITATIONS {
            return Err(AccessError::Capacity);
        }
        // Public IDs use independent randomness and are never derived from a bearer secret.
        let invitation_id = SecretToken::generate()?.expose_secret().to_owned();
        if self.pending.contains_key(&invitation_id) {
            return Err(AccessError::Rejected);
        }
        let secret = SecretToken::generate()?;
        self.pending.insert(
            invitation_id.clone(),
            PendingInvitation {
                verifier: secret.digest(SecretPurpose::Invitation),
                issued_at: now,
                expires_at,
            },
        );
        Ok(IssuedInvitation {
            brain_id: self.brain_id.clone(),
            invitation_id,
            expires_at,
            secret,
        })
    }

    /// Caller must serialize this operation with all credential and membership mutations.
    /// R3.3 persistence must commit consumption, membership and credential issuance atomically.
    pub(super) fn redeem(
        &mut self,
        request: JoinRequest<'_>,
        credentials: &mut CredentialRegistry,
        now: i64,
    ) -> Result<Enrollment, AccessError> {
        self.clock.observe(now)?;
        if request.brain_id != self.brain_id
            || ![
                request.invitation_id,
                request.identity_id,
                request.device_id,
            ]
            .into_iter()
            .all(id)
            || request.display_name.len() > 256
            || request.display_name.trim().is_empty()
            || request.display_name.chars().any(char::is_control)
        {
            return Err(AccessError::Rejected);
        }
        let invitation = self
            .pending
            .get(request.invitation_id)
            .ok_or(AccessError::Rejected)?;
        if now < invitation.issued_at
            || now >= invitation.expires_at
            || !request
                .secret
                .matches(&invitation.verifier, SecretPurpose::Invitation)
        {
            return Err(AccessError::Rejected);
        }
        let member_id = format!("member-{}", SecretToken::generate()?.expose_secret());
        let node_id = format!("node-{}", SecretToken::generate()?.expose_secret());
        let credential = credentials.issue(
            CredentialBinding {
                brain_id: self.brain_id.clone(),
                member_id: member_id.clone(),
                node_id,
                device_id: request.device_id.to_owned(),
            },
            now,
        )?;
        let member = JoinedMember {
            member_id,
            identity_id: request.identity_id.to_owned(),
            display_name: request.display_name.to_owned(),
            role: JoinedRole::Member,
        };
        // Consume only after issuance succeeds; a failed join leaves the invitation usable.
        self.pending.remove(request.invitation_id);
        Ok(Enrollment { member, credential })
    }

    /// Trusted Brain administration; cancelling an invitation does not revoke an enrolled member.
    pub(super) fn cancel(&mut self, invitation_id: &str) -> Result<(), AccessError> {
        self.pending
            .remove(invitation_id)
            .map(|_| ())
            .ok_or(AccessError::Rejected)
    }
}

#[cfg(test)]
#[path = "invitation_tests.rs"]
mod tests;
