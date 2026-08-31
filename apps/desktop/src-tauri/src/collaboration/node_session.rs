use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, TryLockError};

use super::client::{Client, ClientError};
use super::credential::{CredentialBinding, SecretToken};
use super::invitation::JoinRequest;
use super::node_registration::{NodeRegistration, RegistrationError, RegistrationStore};
use super::secret_store::{NodeSecrets, SecretBackend};
use crate::identity::RivloomIdentity;

/// Explicit desktop Node lifecycle. No discovery, background reconnect, enrollment retry, or Run API.
pub(super) struct NodeSession<B> {
    pub(super) store: RegistrationStore,
    pub(super) vault: NodeSecrets<B>,
    state: Mutex<SessionState>,
}

struct SessionState {
    client: Option<Client>,
    closed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NodeStatus {
    state: ConnectionState,
    registration: Option<NodeRegistration>,
    binding: Option<CredentialBinding>,
    revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum ConnectionState {
    NotConfigured,
    RecoveryRequired,
    Disconnected,
    Connected,
}

impl<B: SecretBackend> NodeSession<B> {
    pub(super) fn new(directory: PathBuf, backend: B) -> Result<Self, SessionError> {
        Ok(Self {
            store: RegistrationStore::new(directory)?,
            vault: NodeSecrets::new(backend),
            state: Mutex::new(SessionState {
                client: None,
                closed: false,
            }),
        })
    }

    pub(super) fn status(&self, identity: &RivloomIdentity) -> Result<NodeStatus, SessionError> {
        let state = self.guard()?;
        self.describe(identity, &state)
    }

    /// Persistence before the first network request prevents ambiguous Join outcomes being retried.
    pub(super) fn join(
        &self,
        identity: &RivloomIdentity,
        registration: &NodeRegistration,
        invitation_id: &str,
        secret: &SecretToken,
    ) -> Result<NodeStatus, SessionError> {
        let mut state = self.guard()?;
        registration.check_identity(identity)?;
        SecretToken::parse(invitation_id).map_err(|_| SessionError::Invalid)?;
        let peer = registration
            .trusted_peer()?
            .peer()
            .map_err(|_| SessionError::Invalid)?;
        self.store.begin(registration)?;
        let client = Client::join(
            &peer,
            &self.vault,
            JoinRequest {
                brain_id: registration.descriptor.brain_id(),
                invitation_id,
                secret,
                identity_id: &identity.identity_id,
                device_id: &identity.device_id,
                display_name: &identity.display_name,
            },
        )
        .map_err(SessionError::from)?;
        self.store.complete(registration, client.binding())?;
        state.client = Some(client);
        self.describe(identity, &state)
    }

    pub(super) fn connect(&self, identity: &RivloomIdentity) -> Result<NodeStatus, SessionError> {
        let mut state = self.guard()?;
        let registration = self.store.load(identity)?;
        let binding = self.store.binding(&registration)?;
        let peer = registration
            .trusted_peer()?
            .peer()
            .map_err(|_| SessionError::Invalid)?;
        match state.client.as_mut() {
            Some(client) => {
                if client.binding() != &binding {
                    return Err(SessionError::Invalid);
                }
                client.reconnect(&peer, &self.vault)?;
            }
            None => state.client = Some(Client::connect(&peer, &self.vault, binding)?),
        }
        self.describe(identity, &state)
    }

    /// The desktop supplies only its running managed Brain profile, never an IPC profile/binding.
    pub(super) fn connect_owner(
        &self,
        identity: &RivloomIdentity,
        profile: &super::host_profile::HostProfile,
        confirmed_fingerprint: &str,
    ) -> Result<NodeStatus, SessionError> {
        let mut state = self.guard()?;
        match self.store.load(identity) {
            Err(RegistrationError::NotConfigured) => {}
            Ok(_) => return Err(SessionError::Existing),
            Err(error) => return Err(error.into()),
        }
        let registration = NodeRegistration::confirmed(
            identity,
            &profile
                .descriptor
                .encode()
                .map_err(|_| SessionError::Invalid)?,
            confirmed_fingerprint,
        )?;
        if profile.binding.device_id != identity.device_id {
            return Err(SessionError::Invalid);
        }
        let peer = registration
            .trusted_peer()?
            .peer()
            .map_err(|_| SessionError::Invalid)?;
        // Authenticate the existing protected owner credential before registering any local binding.
        let client = Client::connect(&peer, &self.vault, profile.binding.clone())?;
        if !client.view().shared_records().any(|record| matches!(&record.data,
            super::reconcile::SharedData::Member { member_id, identity_id, owner: true, revoked: false, .. }
                if member_id == &profile.binding.member_id && identity_id == &identity.identity_id)) {
            return Err(SessionError::Invalid);
        }
        self.store.begin(&registration)?;
        self.store.complete(&registration, client.binding())?;
        state.client = Some(client);
        self.describe(identity, &state)
    }

    /// Explicit refresh renews presence and reads a completed projection, not a liveness promise.
    pub(super) fn refresh(&self, identity: &RivloomIdentity) -> Result<NodeStatus, SessionError> {
        self.with_client(identity, |client| client.pulse())?;
        self.status(identity)
    }

    pub(super) fn with_client<T>(
        &self,
        identity: &RivloomIdentity,
        operation: impl FnOnce(&mut Client) -> Result<T, ClientError>,
    ) -> Result<T, SessionError> {
        let mut state = self.guard()?;
        let registration = self.store.load(identity)?;
        let binding = self.store.binding(&registration)?;
        let client = state.client.as_mut().ok_or(SessionError::Disconnected)?;
        if client.binding() != &binding {
            return Err(SessionError::Invalid);
        }
        operation(client).map_err(Into::into)
    }

    pub(super) fn disconnect(&self) -> Result<(), SessionError> {
        if let Some(client) = &mut self.guard()?.client {
            client.disconnect();
        }
        Ok(())
    }

    pub(super) fn shutdown(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed = true;
        state.client.take();
    }

    fn describe(
        &self,
        identity: &RivloomIdentity,
        state: &SessionState,
    ) -> Result<NodeStatus, SessionError> {
        let registration = match self.store.load(identity) {
            Ok(registration) => registration,
            Err(RegistrationError::NotConfigured) => {
                return Ok(NodeStatus {
                    state: ConnectionState::NotConfigured,
                    registration: None,
                    binding: None,
                    revision: 0,
                });
            }
            Err(error) => return Err(error.into()),
        };
        let binding = match self.store.binding(&registration) {
            Ok(binding) => binding,
            Err(RegistrationError::Incomplete) => {
                return Ok(NodeStatus {
                    state: ConnectionState::RecoveryRequired,
                    registration: Some(registration),
                    binding: None,
                    revision: 0,
                });
            }
            Err(error) => return Err(error.into()),
        };
        if state
            .client
            .as_ref()
            .is_some_and(|client| client.binding() != &binding)
        {
            return Err(SessionError::Invalid);
        }
        let ready = state
            .client
            .as_ref()
            .is_some_and(|client| client.view().is_ready());
        Ok(NodeStatus {
            state: if ready {
                ConnectionState::Connected
            } else {
                ConnectionState::Disconnected
            },
            registration: Some(registration),
            binding: Some(binding),
            revision: state
                .client
                .as_ref()
                .map_or(0, |client| client.view().revision()),
        })
    }

    fn guard(&self) -> Result<MutexGuard<'_, SessionState>, SessionError> {
        let state = self.state.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => SessionError::Busy,
            TryLockError::Poisoned(_) => SessionError::Unavailable,
        })?;
        if state.closed {
            return Err(SessionError::Unavailable);
        }
        Ok(state)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SessionError {
    #[error("Invalid Node configuration")]
    Invalid,
    #[error("Node has not been registered")]
    NotConfigured,
    #[error("Node registration requires explicit recovery")]
    RecoveryRequired,
    #[error("Node registration already exists")]
    Existing,
    #[error("Node registration storage unavailable")]
    Storage,
    #[error("Node connection is busy")]
    Busy,
    #[error("Node is disconnected")]
    Disconnected,
    #[error("Node transport failed; operation may require reconciliation")]
    Transport,
    #[error("Node credential unavailable or expired")]
    Credential,
    #[error("Brain rejected the operation")]
    Rejected,
    #[error("Node service unavailable")]
    Unavailable,
}

impl From<RegistrationError> for SessionError {
    fn from(error: RegistrationError) -> Self {
        match error {
            RegistrationError::Invalid => Self::Invalid,
            RegistrationError::NotConfigured => Self::NotConfigured,
            RegistrationError::Incomplete => Self::RecoveryRequired,
            RegistrationError::Existing => Self::Existing,
            RegistrationError::Storage => Self::Storage,
        }
    }
}
impl From<ClientError> for SessionError {
    fn from(error: ClientError) -> Self {
        match error {
            ClientError::Invalid => Self::Invalid,
            ClientError::Transport => Self::Transport,
            ClientError::Peer(_) => Self::Rejected,
            ClientError::Node(_) => Self::Disconnected,
            ClientError::Vault(_) => Self::Credential,
        }
    }
}

#[cfg(test)]
#[path = "node_session_tests.rs"]
mod tests;
