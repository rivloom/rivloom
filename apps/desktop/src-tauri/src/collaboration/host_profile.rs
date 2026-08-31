use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use super::credential::CredentialBinding;
use super::hosting::HostingError;
use super::node::Node;
use super::protocol::timestamp;
use super::trust::TrustDescriptor;

const MAX_PROFILE_BYTES: usize = 12 * 1024;
const FILE: &str = "host-v1.json";

/// Public local registration, committed last during provisioning. Never contains secret key material.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HostProfile {
    pub(super) version: u32,
    pub(super) binding: CredentialBinding,
    pub(super) descriptor: TrustDescriptor,
    pub(super) credential_expires_at: i64,
}

impl HostProfile {
    fn validate(&self) -> Result<(), HostingError> {
        if self.version != 1
            || self.binding.brain_id != self.descriptor.brain_id()
            || !timestamp(self.credential_expires_at)
        {
            return Err(HostingError::Invalid);
        }
        Node::new(self.binding.clone()).map_err(|_| HostingError::Invalid)?;
        self.descriptor
            .encode()
            .map_err(|_| HostingError::Invalid)?;
        Ok(())
    }

    pub(super) fn load(directory: &Path) -> Result<Self, HostingError> {
        let metadata = fs::symlink_metadata(directory).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                HostingError::NotConfigured
            } else {
                HostingError::Storage
            }
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(HostingError::Invalid);
        }
        let path = directory.join(FILE);
        let metadata = fs::symlink_metadata(&path).map_err(|_| HostingError::Incomplete)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_PROFILE_BYTES as u64
        {
            return Err(HostingError::Invalid);
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|_| HostingError::Storage)?
            .take(MAX_PROFILE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| HostingError::Storage)?;
        if bytes.len() > MAX_PROFILE_BYTES {
            return Err(HostingError::Invalid);
        }
        let profile: Self = serde_json::from_slice(&bytes).map_err(|_| HostingError::Invalid)?;
        profile.validate()?;
        Ok(profile)
    }

    pub(super) fn write_new(&self, directory: &Path) -> Result<(), HostingError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| HostingError::Invalid)?;
        if bytes.len() > MAX_PROFILE_BYTES {
            return Err(HostingError::Invalid);
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(directory.join(FILE))
            .map_err(|_| HostingError::Storage)?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| HostingError::Storage)
    }
}
