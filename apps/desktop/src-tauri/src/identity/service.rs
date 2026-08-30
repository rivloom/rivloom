use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use thiserror::Error;

use super::storage::IdentityStore;
use super::storage::StorageError;
use super::types::BrainMembershipRole;
use super::types::RivloomIdentity;

pub(crate) const DEFAULT_DISPLAY_NAME: &str = "本机用户";
const MAX_DISPLAY_NAME_BYTES: usize = 80;
const GENERATED_ID_HEX_BYTES: usize = 32;
const MAX_MEMBERSHIP_ID_BYTES: usize = 128;
static ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct IdentityService {
    store: IdentityStore,
    generator: Arc<dyn IdentityIdGenerator>,
    identity: Mutex<Option<RivloomIdentity>>,
}

impl IdentityService {
    pub(crate) fn new(store: IdentityStore) -> Self {
        Self::with_generator(store, Arc::new(SystemIdentityIdGenerator))
    }

    fn with_generator(store: IdentityStore, generator: Arc<dyn IdentityIdGenerator>) -> Self {
        Self {
            store,
            generator,
            identity: Mutex::new(None),
        }
    }

    pub(crate) fn get(&self) -> Result<RivloomIdentity, IdentityServiceError> {
        let mut identity = self
            .identity
            .lock()
            .map_err(|_| IdentityServiceError::State)?;
        self.load_or_create(&mut identity)
    }

    pub(crate) fn update_display_name(
        &self,
        display_name: &str,
    ) -> Result<RivloomIdentity, IdentityServiceError> {
        let display_name = normalize_display_name(display_name)?;
        let mut cached = self
            .identity
            .lock()
            .map_err(|_| IdentityServiceError::State)?;
        let mut identity = self.load_or_create(&mut cached)?;
        identity.display_name = display_name;
        self.store.save(&identity)?;
        *cached = Some(identity.clone());
        Ok(identity)
    }

    fn load_or_create(
        &self,
        cached: &mut Option<RivloomIdentity>,
    ) -> Result<RivloomIdentity, IdentityServiceError> {
        if let Some(identity) = cached {
            return Ok(identity.clone());
        }
        let identity = match self.store.load()? {
            Some(identity) => {
                validate_stored_identity(&identity)?;
                identity
            }
            None => {
                let identity = RivloomIdentity {
                    identity_id: self.generator.generate(IdentityIdKind::Identity),
                    display_name: DEFAULT_DISPLAY_NAME.to_string(),
                    device_id: self.generator.generate(IdentityIdKind::Device),
                    brain_membership: None,
                };
                self.store.save(&identity)?;
                identity
            }
        };
        *cached = Some(identity.clone());
        Ok(identity)
    }
}

fn normalize_display_name(display_name: &str) -> Result<String, IdentityServiceError> {
    let normalized = display_name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() || normalized.len() > MAX_DISPLAY_NAME_BYTES {
        return Err(IdentityServiceError::InvalidDisplayName);
    }
    Ok(normalized)
}

fn validate_stored_identity(identity: &RivloomIdentity) -> Result<(), IdentityServiceError> {
    if !valid_generated_id(&identity.identity_id, "identity-v1-")
        || !valid_generated_id(&identity.device_id, "device-v1-")
        || normalize_display_name(&identity.display_name).as_deref()
            != Ok(identity.display_name.as_str())
    {
        return Err(IdentityServiceError::InvalidStoredIdentity);
    }
    if let Some(membership) = &identity.brain_membership {
        if !valid_membership_id(&membership.brain_id) || !valid_membership_id(&membership.member_id)
        {
            return Err(IdentityServiceError::InvalidStoredIdentity);
        }
        match membership.role {
            BrainMembershipRole::Owner | BrainMembershipRole::Member => {}
        }
    }
    Ok(())
}

fn valid_generated_id(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == GENERATED_ID_HEX_BYTES
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn valid_membership_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MEMBERSHIP_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum IdentityIdKind {
    Identity,
    Device,
}

trait IdentityIdGenerator: Send + Sync {
    fn generate(&self, kind: IdentityIdKind) -> String;
}

#[derive(Debug)]
struct SystemIdentityIdGenerator;

impl IdentityIdGenerator for SystemIdentityIdGenerator {
    fn generate(&self, kind: IdentityIdKind) -> String {
        let prefix = match kind {
            IdentityIdKind::Identity => "identity-v1-",
            IdentityIdKind::Device => "device-v1-",
        };
        let sequence = ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut first = RandomState::new().build_hasher();
        kind.hash(&mut first);
        now.hash(&mut first);
        std::process::id().hash(&mut first);
        sequence.hash(&mut first);
        std::thread::current().id().hash(&mut first);
        let first = first.finish();
        let mut second = RandomState::new().build_hasher();
        kind.hash(&mut second);
        first.hash(&mut second);
        sequence.hash(&mut second);
        let second = second.finish();
        format!("{prefix}{first:016x}{second:016x}")
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum IdentityServiceError {
    #[error("identity storage is unavailable")]
    Storage,
    #[error("display name is empty or too long")]
    InvalidDisplayName,
    #[error("stored identity is invalid")]
    InvalidStoredIdentity,
    #[error("identity state is unavailable")]
    State,
}

impl From<StorageError> for IdentityServiceError {
    fn from(_error: StorageError) -> Self {
        Self::Storage
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
