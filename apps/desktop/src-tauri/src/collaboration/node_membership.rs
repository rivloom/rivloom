use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroizing;

use super::client::ClientError;
use super::credential::SecretToken;
use super::node::NodeError;
use super::node_session::{NodeSession, SessionError};
use super::reconcile::SharedData;
use super::secret_store::SecretBackend;
use crate::identity::RivloomIdentity;

/// Invitation-only IPC field: short-lived user transfer, never Node/TLS key serialization.
/// Frontends must keep it transient and must not log or persist it. Debug remains redacted.
#[derive(Debug)]
pub(crate) struct InvitationSecret(pub(super) SecretToken);

impl Serialize for InvitationSecret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.expose_secret())
    }
}
impl<'de> Deserialize<'de> for InvitationSecret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Zeroizing::new(String::deserialize(deserializer)?);
        SecretToken::parse(&value)
            .map(Self)
            .map_err(|_| serde::de::Error::custom("Invalid invitation code"))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InvitationDisplay {
    brain_id: String,
    invitation_id: String,
    expires_at: i64,
    secret: InvitationSecret,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemberDirectory {
    revision: u64,
    entries: Vec<DirectoryEntry>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum DirectoryEntry {
    Member {
        member_id: String,
        display_name: String,
        owner: bool,
        revoked: bool,
    },
    Node {
        node_id: String,
        member_id: String,
        online: bool,
        last_seen_at: Option<i64>,
    },
}

impl<B: SecretBackend> NodeSession<B> {
    /// Last completed projection only; no tasks, announcements, credentials, or local paths.
    pub(super) fn members(
        &self,
        identity: &RivloomIdentity,
    ) -> Result<MemberDirectory, SessionError> {
        self.with_client(identity, |client| {
            if !client.view().is_ready() {
                return Err(ClientError::Node(NodeError::Unavailable));
            }
            let entries = client
                .view()
                .shared_records()
                .filter_map(|record| match &record.data {
                    SharedData::Member {
                        member_id,
                        display_name,
                        owner,
                        revoked,
                        ..
                    } => Some(DirectoryEntry::Member {
                        member_id: member_id.clone(),
                        display_name: display_name.clone(),
                        owner: *owner,
                        revoked: *revoked,
                    }),
                    SharedData::Node {
                        node_id,
                        member_id,
                        online,
                        last_seen_at,
                        ..
                    } => Some(DirectoryEntry::Node {
                        node_id: node_id.clone(),
                        member_id: member_id.clone(),
                        online: *online,
                        last_seen_at: *last_seen_at,
                    }),
                    SharedData::Task { .. } => None,
                })
                .collect::<Vec<_>>();
            let directory = MemberDirectory {
                revision: client.view().revision(),
                entries,
            };
            if directory.entries.len() > 128
                || serde_json::to_vec(&directory)
                    .map_err(|_| ClientError::Invalid)?
                    .len()
                    > 64 * 1024
            {
                return Err(ClientError::Invalid);
            }
            Ok(directory)
        })
    }

    /// Every invocation creates a new invitation; never retry automatically after an uncertain reply.
    pub(super) fn invite(
        &self,
        identity: &RivloomIdentity,
    ) -> Result<InvitationDisplay, SessionError> {
        let invitation = self.with_client(identity, |client| client.invite())?;
        Ok(InvitationDisplay {
            brain_id: invitation.brain_id,
            invitation_id: invitation.invitation_id,
            expires_at: invitation.expires_at,
            secret: InvitationSecret(invitation.secret),
        })
    }

    pub(super) fn cancel_invite(
        &self,
        identity: &RivloomIdentity,
        invitation_id: String,
    ) -> Result<(), SessionError> {
        SecretToken::parse(&invitation_id).map_err(|_| SessionError::Invalid)?;
        self.with_client(identity, |client| client.cancel_invite(invitation_id))
    }

    pub(super) fn revoke(
        &self,
        identity: &RivloomIdentity,
        member_id: String,
    ) -> Result<(), SessionError> {
        self.with_client(identity, |client| client.revoke(member_id))
    }
}

#[cfg(test)]
#[path = "node_membership_tests.rs"]
mod tests;
