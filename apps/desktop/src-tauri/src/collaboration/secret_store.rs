use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::credential::{CredentialBinding, IssuedCredential, SecretToken};
use super::protocol::{id, timestamp};

const MAX_SECRET_BYTES: usize = 1024;
const PREFIX: &str = "Rivloom/node/v1/";

/// Explicit secret field for OS vault blobs and established TLS frames only, never UI/state DTOs.
#[derive(Debug)]
pub(super) struct SecretField(pub(super) SecretToken);

impl Serialize for SecretField {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.expose_secret())
    }
}
impl<'de> Deserialize<'de> for SecretField {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Zeroizing::new(String::deserialize(deserializer)?);
        SecretToken::parse(&value)
            .map(Self)
            .map_err(|_| serde::de::Error::custom("Invalid collaboration secret"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    version: u32,
    binding: CredentialBinding,
    expires_at: i64,
    secret: SecretField,
}

/// Store only app-namespaced blobs, never log them, and return zeroizing owned read buffers.
/// Native implementations must use OS secret protection and must not fall back to plaintext files.
pub(super) trait SecretBackend {
    fn read(&self, target: &str) -> Result<Option<Zeroizing<Vec<u8>>>, VaultError>;
    fn write_new(&self, target: &str, bytes: &[u8]) -> Result<(), VaultError>;
    fn remove(&self, target: &str) -> Result<(), VaultError>;
}

pub(super) struct NodeSecrets<B> {
    backend: B,
}

impl<B: SecretBackend> NodeSecrets<B> {
    pub(super) fn new(backend: B) -> Self {
        Self { backend }
    }
    pub(super) fn save_new(
        &self,
        credential: &IssuedCredential,
        now: i64,
    ) -> Result<(), VaultError> {
        let slot = target(&credential.binding)?;
        if !timestamp(now)
            || !timestamp(credential.expires_at)
            || credential.expires_at <= now
            || credential.expires_at - now > 86400
        {
            return Err(VaultError::Invalid);
        }
        let document = Document {
            version: 1,
            binding: credential.binding.clone(),
            expires_at: credential.expires_at,
            secret: SecretField(
                SecretToken::parse(credential.secret.expose_secret())
                    .map_err(|_| VaultError::Invalid)?,
            ),
        };
        let bytes = Zeroizing::new(serde_json::to_vec(&document).map_err(|_| VaultError::Invalid)?);
        if bytes.len() > MAX_SECRET_BYTES {
            return Err(VaultError::Invalid);
        }
        self.backend.write_new(&slot, &bytes)
    }
    pub(super) fn load(
        &self,
        binding: &CredentialBinding,
        now: i64,
    ) -> Result<IssuedCredential, VaultError> {
        let bytes = self
            .backend
            .read(&target(binding)?)?
            .ok_or(VaultError::Missing)?;
        if bytes.len() > MAX_SECRET_BYTES {
            return Err(VaultError::Invalid);
        }
        let document: Document = serde_json::from_slice(&bytes).map_err(|_| VaultError::Invalid)?;
        if document.version != 1
            || &document.binding != binding
            || !timestamp(now)
            || !timestamp(document.expires_at)
            || document.expires_at <= now
        {
            return Err(VaultError::Invalid);
        }
        Ok(IssuedCredential {
            binding: document.binding,
            expires_at: document.expires_at,
            secret: document.secret.0,
        })
    }
    pub(super) fn remove(&self, binding: &CredentialBinding) -> Result<(), VaultError> {
        self.backend.remove(&target(binding)?)
    }
}

fn target(binding: &CredentialBinding) -> Result<String, VaultError> {
    if [
        &binding.brain_id,
        &binding.member_id,
        &binding.node_id,
        &binding.device_id,
    ]
    .into_iter()
    .any(|v| !id(v))
    {
        return Err(VaultError::Invalid);
    }
    // Windows target names are case-insensitive; hash the exact case-sensitive full binding.
    let bytes = serde_json::to_vec(binding).map_err(|_| VaultError::Invalid)?;
    Ok(format!("{PREFIX}{:x}", Sha256::digest(bytes)))
}

pub(super) struct NativeVault;

#[cfg(windows)]
mod windows {
    use super::*;
    use std::sync::Mutex;
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    };
    use zeroize::Zeroize;

    static WRITER: Mutex<()> = Mutex::new(());

    fn wide(target: &str) -> Result<Vec<u16>, VaultError> {
        if target.len() != PREFIX.len() + 64
            || !target.starts_with(PREFIX)
            || !target[PREFIX.len()..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(VaultError::Invalid);
        }
        Ok(target.encode_utf16().chain([0]).collect())
    }

    impl SecretBackend for NativeVault {
        fn read(&self, target: &str) -> Result<Option<Zeroizing<Vec<u8>>>, VaultError> {
            let target = wide(target)?;
            let mut pointer = std::ptr::null_mut();
            // SAFETY: The NUL-terminated name and output pointer live for the call.
            if unsafe {
                CredReadW(
                    target.as_ptr(),
                    CRED_TYPE_GENERIC,
                    /*flags*/ 0,
                    &mut pointer,
                )
            } == 0
            {
                // SAFETY: GetLastError has no preconditions and is read immediately after failure.
                return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                    Ok(None)
                } else {
                    Err(VaultError::Unavailable)
                };
            }
            if pointer.is_null() {
                return Err(VaultError::Unavailable);
            }
            // SAFETY: Successful CredReadW owns one allocated block, including its blob, until CredFree.
            let record = unsafe { &mut *pointer };
            let length = record.CredentialBlobSize as usize;
            let result = if record.Type != CRED_TYPE_GENERIC
                || record.Persist != CRED_PERSIST_LOCAL_MACHINE
                || length == 0
                || length > MAX_SECRET_BYTES
                || record.CredentialBlob.is_null()
            {
                Err(VaultError::Invalid)
            } else {
                // SAFETY: The OS returned a readable/writable blob of exactly the checked length.
                let blob = unsafe { std::slice::from_raw_parts_mut(record.CredentialBlob, length) };
                let bytes = Zeroizing::new(blob.to_vec());
                blob.zeroize();
                Ok(Some(bytes))
            };
            // SAFETY: Free exactly the block returned by CredReadW; no borrowed pointer escapes.
            unsafe { CredFree(pointer.cast()) };
            result
        }
        fn write_new(&self, target: &str, bytes: &[u8]) -> Result<(), VaultError> {
            let _guard = WRITER.lock().map_err(|_| VaultError::Unavailable)?;
            if bytes.is_empty() || bytes.len() > MAX_SECRET_BYTES {
                return Err(VaultError::Invalid);
            }
            if self.read(target)?.is_some() {
                return Err(VaultError::Existing);
            }
            let mut target = wide(target)?;
            let record = CREDENTIALW {
                Type: CRED_TYPE_GENERIC,
                TargetName: target.as_mut_ptr(),
                CredentialBlobSize: bytes.len() as u32,
                CredentialBlob: bytes.as_ptr().cast_mut(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                ..Default::default()
            };
            // SAFETY: All input buffers remain valid; CredWriteW copies the bounded blob during this call.
            if unsafe {
                CredWriteW(&record, /*flags*/ 0)
            } == 0
            {
                return Err(VaultError::Unavailable);
            }
            Ok(())
        }
        fn remove(&self, target: &str) -> Result<(), VaultError> {
            let _guard = WRITER.lock().map_err(|_| VaultError::Unavailable)?;
            let target = wide(target)?;
            // SAFETY: Delete only this validated app-namespaced, NUL-terminated target.
            if unsafe {
                CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, /*flags*/ 0)
            } == 0
            {
                // SAFETY: Read this call's error before another OS call.
                if unsafe { GetLastError() } != ERROR_NOT_FOUND {
                    return Err(VaultError::Unavailable);
                }
            }
            Ok(())
        }
    }
}

#[cfg(not(windows))]
impl SecretBackend for NativeVault {
    fn read(&self, _target: &str) -> Result<Option<Zeroizing<Vec<u8>>>, VaultError> {
        Err(VaultError::Unavailable)
    }
    fn write_new(&self, _target: &str, _bytes: &[u8]) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
    fn remove(&self, _target: &str) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum VaultError {
    #[error("Invalid protected Node credential")]
    Invalid,
    #[error("A protected Node credential already exists")]
    Existing,
    #[error("Protected Node credential missing")]
    Missing,
    #[error("OS Node credential protection unavailable")]
    Unavailable,
}

#[cfg(test)]
#[path = "secret_store_tests.rs"]
mod tests;
