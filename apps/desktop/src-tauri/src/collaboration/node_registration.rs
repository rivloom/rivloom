use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::credential::CredentialBinding;
use super::node::Node;
use super::protocol::id;
use super::trust::{TrustDescriptor, TrustedPeer};
use crate::identity::RivloomIdentity;

/// Immutable, public record of an explicit trust decision. Never stores invitation or Node secrets.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct NodeRegistration {
    version: u32,
    identity_id: String,
    device_id: String,
    pub(super) descriptor: TrustDescriptor,
    confirmed_fingerprint: String,
}

impl NodeRegistration {
    pub(super) fn confirmed(
        identity: &RivloomIdentity,
        descriptor: &[u8],
        confirmed_fingerprint: &str,
    ) -> Result<Self, RegistrationError> {
        let trusted = TrustedPeer::confirm(descriptor, confirmed_fingerprint)
            .map_err(|_| RegistrationError::Invalid)?;
        let registration = Self {
            version: 1,
            identity_id: identity.identity_id.clone(),
            device_id: identity.device_id.clone(),
            descriptor: trusted.descriptor().clone(),
            confirmed_fingerprint: confirmed_fingerprint.into(),
        };
        registration.validate()?;
        Ok(registration)
    }

    fn validate(&self) -> Result<(), RegistrationError> {
        if self.version != 1 || !id(&self.identity_id) || !id(&self.device_id) {
            return Err(RegistrationError::Invalid);
        }
        self.trusted_peer().map(|_| ())
    }

    pub(super) fn check_identity(
        &self,
        identity: &RivloomIdentity,
    ) -> Result<(), RegistrationError> {
        if self.identity_id != identity.identity_id || self.device_id != identity.device_id {
            return Err(RegistrationError::Invalid);
        }
        Ok(())
    }

    /// Restores the previously recorded confirmation; never derives a replacement confirmation.
    pub(super) fn trusted_peer(&self) -> Result<TrustedPeer, RegistrationError> {
        TrustedPeer::confirm(
            &self
                .descriptor
                .encode()
                .map_err(|_| RegistrationError::Invalid)?,
            &self.confirmed_fingerprint,
        )
        .map_err(|_| RegistrationError::Invalid)
    }

    fn validate_binding(&self, binding: &CredentialBinding) -> Result<(), RegistrationError> {
        Node::new(binding.clone()).map_err(|_| RegistrationError::Invalid)?;
        if binding.brain_id != self.descriptor.brain_id() || binding.device_id != self.device_id {
            return Err(RegistrationError::Invalid);
        }
        Ok(())
    }
}

/// One enrollment directory per desktop. Registration is durable before any Join request;
/// a separate create-new binding marks success. A partial attempt cannot silently enroll again.
pub(super) struct RegistrationStore {
    directory: PathBuf,
}

impl RegistrationStore {
    pub(super) fn new(directory: PathBuf) -> Result<Self, RegistrationError> {
        if !directory.is_absolute() || directory.file_name().is_none() {
            return Err(RegistrationError::Invalid);
        }
        Ok(Self { directory })
    }

    pub(super) fn begin(&self, registration: &NodeRegistration) -> Result<(), RegistrationError> {
        registration.validate()?;
        fs::create_dir_all(self.directory.parent().ok_or(RegistrationError::Invalid)?)
            .map_err(|_| RegistrationError::Storage)?;
        fs::create_dir(&self.directory).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                RegistrationError::Existing
            } else {
                RegistrationError::Storage
            }
        })?;
        write_new(&self.directory.join("registration-v1.json"), registration)
    }

    pub(super) fn load(
        &self,
        identity: &RivloomIdentity,
    ) -> Result<NodeRegistration, RegistrationError> {
        let registration = self.read_registration()?;
        registration.check_identity(identity)?;
        Ok(registration)
    }

    fn read_registration(&self) -> Result<NodeRegistration, RegistrationError> {
        let metadata = fs::symlink_metadata(&self.directory).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                RegistrationError::NotConfigured
            } else {
                RegistrationError::Storage
            }
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(RegistrationError::Invalid);
        }
        let registration: NodeRegistration =
            read(&self.directory.join("registration-v1.json"), 12 * 1024)?;
        registration.validate()?;
        Ok(registration)
    }

    pub(super) fn complete(
        &self,
        registration: &NodeRegistration,
        binding: &CredentialBinding,
    ) -> Result<(), RegistrationError> {
        if self.read_registration()? != *registration {
            return Err(RegistrationError::Invalid);
        }
        registration.validate_binding(binding)?;
        write_new(&self.directory.join("binding-v1.json"), binding)
    }

    pub(super) fn binding(
        &self,
        registration: &NodeRegistration,
    ) -> Result<CredentialBinding, RegistrationError> {
        if self.read_registration()? != *registration {
            return Err(RegistrationError::Invalid);
        }
        let binding = read(&self.directory.join("binding-v1.json"), 1024)?;
        registration.validate_binding(&binding)?;
        Ok(binding)
    }
}

fn read<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T, RegistrationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| RegistrationError::Incomplete)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit {
        return Err(RegistrationError::Invalid);
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| RegistrationError::Storage)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| RegistrationError::Storage)?;
    if bytes.len() as u64 > limit {
        return Err(RegistrationError::Invalid);
    }
    serde_json::from_slice(&bytes).map_err(|_| RegistrationError::Invalid)
}

fn write_new<T: Serialize>(path: &Path, value: &T) -> Result<(), RegistrationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| RegistrationError::Invalid)?;
    if bytes.len() > 12 * 1024 {
        return Err(RegistrationError::Invalid);
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            RegistrationError::Existing
        } else {
            RegistrationError::Storage
        }
    })?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| RegistrationError::Storage)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum RegistrationError {
    #[error("Invalid Node registration")]
    Invalid,
    #[error("Node has not been registered")]
    NotConfigured,
    #[error("Node registration requires explicit recovery")]
    Incomplete,
    #[error("Node registration already exists")]
    Existing,
    #[error("Node registration storage unavailable")]
    Storage,
}

#[cfg(test)]
#[path = "node_registration_tests.rs"]
mod tests;
