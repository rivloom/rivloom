use rustls::pki_types::ServerName;
use serde::Serialize;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use super::brain::OwnerProfile;
use super::credential::SecretToken;
use super::host::Host;
use super::host_profile::HostProfile;
use super::secret_store::{NodeSecrets, SecretBackend};
use super::server::{Server, now};
use super::server_identity::ServerIdentityStore;
use super::storage::BrainStore;
use super::tls::private_address;
use crate::identity::RivloomIdentity;

pub(super) struct BrainService<B> {
    directory: PathBuf,
    backend: Arc<B>,
    state: Mutex<ServiceState>,
}
struct ServiceState {
    running: Option<Running>,
    closed: bool,
}
struct Running {
    server: Server,
    profile: HostProfile,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", content = "profile", rename_all = "camelCase")]
pub(crate) enum HostingStatus {
    NotConfigured,
    Stopped(HostProfile),
    Running(HostProfile),
    Faulted,
}

impl<B: SecretBackend> BrainService<B> {
    /// Directory comes only from the app's local data root, never an IPC path or a peer message.
    pub(super) fn new(directory: PathBuf, backend: B) -> Result<Self, HostingError> {
        if !directory.is_absolute() || directory.file_name().is_none() {
            return Err(HostingError::Invalid);
        }
        Ok(Self {
            directory,
            backend: Arc::new(backend),
            state: Mutex::new(ServiceState {
                running: None,
                closed: false,
            }),
        })
    }

    pub(super) fn status(&self) -> Result<HostingStatus, HostingError> {
        let state = self.guard()?;
        if let Some(running) = &state.running {
            return Ok(if running.server.is_running() {
                HostingStatus::Running(running.profile.clone())
            } else {
                HostingStatus::Faulted
            });
        }
        match HostProfile::load(&self.directory) {
            Ok(profile) => Ok(HostingStatus::Stopped(profile)),
            Err(HostingError::NotConfigured) => Ok(HostingStatus::NotConfigured),
            Err(error) => Err(error),
        }
    }

    /// A successful registration is written last; partial provisioning is retained for explicit recovery.
    pub(super) fn initialize(
        &self,
        identity: &RivloomIdentity,
        address: SocketAddr,
        server_name: &str,
    ) -> Result<HostProfile, HostingError> {
        let _state = self.guard()?;
        if !private_address(address.ip())
            || address.port() == 0
            || server_name.len() > 253
            || ServerName::try_from(server_name).is_err()
        {
            return Err(HostingError::Invalid);
        }
        match fs::symlink_metadata(&self.directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => return Err(HostingError::Existing),
            Err(_) => return Err(HostingError::Storage),
        }
        let time = now().map_err(|_| HostingError::Unavailable)?;
        let brain_id = format!(
            "brain-{}",
            SecretToken::generate()
                .map_err(|_| HostingError::Unavailable)?
                .expose_secret()
        );
        let (_store, credential) = BrainStore::create(
            self.directory.clone(),
            brain_id.clone(),
            OwnerProfile {
                identity_id: &identity.identity_id,
                device_id: &identity.device_id,
                display_name: &identity.display_name,
            },
            time,
        )
        .map_err(|_| HostingError::Storage)?;
        let tls = ServerIdentityStore::new(self.backend.clone())
            .create(&brain_id, address, server_name, time)
            .map_err(|_| HostingError::Credential)?;
        NodeSecrets::new(self.backend.clone())
            .save_new(&credential, time)
            .map_err(|_| HostingError::Credential)?;
        let profile = HostProfile {
            version: 1,
            binding: credential.binding,
            descriptor: tls.descriptor,
            credential_expires_at: credential.expires_at,
        };
        profile.write_new(&self.directory)?;
        Ok(profile)
    }

    pub(super) fn start(&self, identity: &RivloomIdentity) -> Result<HostingStatus, HostingError> {
        let mut state = self.guard()?;
        if state.running.is_some() {
            return Err(HostingError::Busy);
        }
        let profile = HostProfile::load(&self.directory)?;
        if profile.binding.device_id != identity.device_id {
            return Err(HostingError::Invalid);
        }
        let time = now().map_err(|_| HostingError::Unavailable)?;
        let mut store = BrainStore::open(self.directory.clone(), &profile.binding.brain_id, time)
            .map_err(|_| HostingError::Storage)?;
        let brain = store.brain().map_err(|_| HostingError::Storage)?;
        if profile.binding.member_id != brain.state.owner_member_id
            || !brain
                .state
                .members
                .get(&profile.binding.member_id)
                .is_some_and(|member| member.identity_id == identity.identity_id)
        {
            return Err(HostingError::Invalid);
        }
        let credential = NodeSecrets::new(self.backend.clone())
            .load(&profile.binding, time)
            .map_err(|_| HostingError::Credential)?;
        if credential.expires_at != profile.credential_expires_at {
            return Err(HostingError::Invalid);
        }
        store
            .transact(time, |brain| {
                brain.connect(&credential.binding, &credential.secret, time)
            })
            .map_err(|_| HostingError::Credential)?;
        let tls = ServerIdentityStore::new(self.backend.clone())
            .load(
                &profile.binding.brain_id,
                profile.descriptor.address(),
                time,
            )
            .map_err(|_| HostingError::Credential)?;
        if tls.descriptor != profile.descriptor {
            return Err(HostingError::Invalid);
        }
        let server = Server::start(Host::new(store), tls.tls, profile.descriptor.address())
            .map_err(|_| HostingError::Unavailable)?;
        state.running = Some(Running {
            server,
            profile: profile.clone(),
        });
        Ok(HostingStatus::Running(profile))
    }

    pub(super) fn stop(&self) -> Result<(), HostingError> {
        self.guard()?.running.take();
        Ok(())
    }

    /// Exit waits for in-flight provisioning/start, then prevents queued commands from restarting it.
    pub(super) fn shutdown(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed = true;
        state.running.take();
    }

    fn guard(&self) -> Result<MutexGuard<'_, ServiceState>, HostingError> {
        let guard = self.state.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => HostingError::Busy,
            TryLockError::Poisoned(_) => HostingError::Unavailable,
        })?;
        if guard.closed {
            return Err(HostingError::Unavailable);
        }
        Ok(guard)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
pub(crate) enum HostingError {
    #[error("Invalid local Brain configuration")]
    Invalid,
    #[error("Local Brain has not been configured")]
    NotConfigured,
    #[error("Local Brain provisioning is incomplete; explicit recovery is required")]
    Incomplete,
    #[error("Local Brain configuration already exists")]
    Existing,
    #[error("Local Brain is busy")]
    Busy,
    #[error("Local Brain storage is unavailable")]
    Storage,
    #[error("Protected Brain identity is unavailable or expired")]
    Credential,
    #[error("Local Brain service is unavailable")]
    Unavailable,
}

#[cfg(test)]
#[path = "hosting_tests.rs"]
mod tests;
