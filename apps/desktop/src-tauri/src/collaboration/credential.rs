use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use super::protocol::{id, timestamp};

pub(super) const MAX_CREDENTIALS: usize = 64;
const CREDENTIAL_TTL_SECONDS: i64 = 24 * 60 * 60;

pub(super) struct SecretToken(Zeroizing<String>);

impl SecretToken {
    pub(super) fn generate() -> Result<Self, AccessError> {
        let mut bytes = Zeroizing::new([0u8; 32]);
        getrandom::fill(bytes.as_mut()).map_err(|_| AccessError::EntropyUnavailable)?;
        let mut encoded = Zeroizing::new(String::with_capacity(64));
        const HEX: &[u8] = b"0123456789abcdef";
        for byte in bytes.iter() {
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 15) as usize] as char);
        }
        Ok(Self(encoded))
    }

    pub(super) fn parse(value: &str) -> Result<Self, AccessError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(AccessError::Rejected);
        }
        Ok(Self(Zeroizing::new(value.to_owned())))
    }

    /// Only for an explicitly authorized secret display or encrypted authentication transport.
    pub(super) fn expose_secret(&self) -> &str {
        &self.0
    }

    pub(super) fn digest(&self, purpose: SecretPurpose) -> [u8; 32] {
        let domain: &[u8] = match purpose {
            SecretPurpose::NodeCredential => b"rivloom/node-credential/v1\0",
            SecretPurpose::Invitation => b"rivloom/invitation/v1\0",
        };
        let mut hash = Sha256::new();
        hash.update(domain);
        hash.update(self.0.as_bytes());
        hash.finalize().into()
    }

    pub(super) fn matches(&self, expected: &[u8; 32], purpose: SecretPurpose) -> bool {
        bool::from(self.digest(purpose).ct_eq(expected))
    }
}

impl fmt::Debug for SecretToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretToken([REDACTED])")
    }
}

pub(super) enum SecretPurpose {
    NodeCredential,
    Invitation,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct Clock {
    high_water_at: i64,
}

impl Clock {
    /// The caller supplies trusted server time, never a timestamp from a peer.
    pub(super) fn observe(&mut self, now: i64) -> Result<(), AccessError> {
        if !timestamp(now) || now < self.high_water_at {
            return Err(AccessError::Rejected);
        }
        self.high_water_at = now;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CredentialBinding {
    pub(super) brain_id: String,
    pub(super) member_id: String,
    pub(super) node_id: String,
    pub(super) device_id: String,
}

#[derive(Debug)]
pub(super) struct IssuedCredential {
    pub(super) binding: CredentialBinding,
    pub(super) expires_at: i64,
    pub(super) secret: SecretToken,
}

// No Deserialize/Serialize: callers cannot manufacture or persist an authorization decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConnectionIdentity {
    binding: CredentialBinding,
    verifier: [u8; 32],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredCredential {
    binding: CredentialBinding,
    issued_at: i64,
    expires_at: i64,
    verifier: [u8; 32],
    revoked: bool,
}

// Not Clone: all connections and task gates must consult the same live authority.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CredentialRegistry {
    brain_id: String,
    clock: Clock,
    records: BTreeMap<String, StoredCredential>,
}

impl CredentialRegistry {
    pub(super) fn new(brain_id: String) -> Result<Self, AccessError> {
        if !id(&brain_id) {
            return Err(AccessError::Rejected);
        }
        Ok(Self {
            brain_id,
            clock: Clock::default(),
            records: BTreeMap::new(),
        })
    }

    /// Trusted Brain administration only; this does not authorize a remote caller to issue tokens.
    pub(super) fn issue(
        &mut self,
        binding: CredentialBinding,
        now: i64,
    ) -> Result<IssuedCredential, AccessError> {
        self.clock.observe(now)?;
        if binding.brain_id != self.brain_id
            || ![
                &binding.brain_id,
                &binding.member_id,
                &binding.node_id,
                &binding.device_id,
            ]
            .into_iter()
            .all(|v| id(v))
            || self.records.contains_key(&binding.node_id)
            || self
                .records
                .values()
                .any(|record| record.binding.member_id == binding.member_id && record.revoked)
        {
            return Err(AccessError::Rejected);
        }
        if self.records.len() >= MAX_CREDENTIALS {
            return Err(AccessError::Capacity);
        }
        let expires_at = now
            .checked_add(CREDENTIAL_TTL_SECONDS)
            .filter(|value| timestamp(*value))
            .ok_or(AccessError::Rejected)?;
        let secret = SecretToken::generate()?;
        let record = StoredCredential {
            binding: binding.clone(),
            issued_at: now,
            expires_at,
            verifier: secret.digest(SecretPurpose::NodeCredential),
            revoked: false,
        };
        self.records.insert(binding.node_id.clone(), record);
        Ok(IssuedCredential {
            binding,
            expires_at,
            secret,
        })
    }

    pub(super) fn connect(
        &mut self,
        binding: &CredentialBinding,
        secret: &SecretToken,
        now: i64,
    ) -> Result<ConnectionIdentity, AccessError> {
        let record = self.active_record(binding, now)?;
        if !secret.matches(&record.verifier, SecretPurpose::NodeCredential) {
            return Err(AccessError::Rejected);
        }
        Ok(ConnectionIdentity {
            binding: binding.clone(),
            verifier: record.verifier,
        })
    }

    /// Recheck immediately before accepting new work, even for an already authenticated connection.
    pub(super) fn authorize_task(
        &mut self,
        session: &ConnectionIdentity,
        now: i64,
    ) -> Result<(), AccessError> {
        let record = self.active_record(&session.binding, now)?;
        if !bool::from(record.verifier.ct_eq(&session.verifier)) {
            return Err(AccessError::Rejected);
        }
        Ok(())
    }

    pub(super) fn revoke_member(&mut self, member_id: &str) -> Result<(), AccessError> {
        let mut found = false;
        for record in self
            .records
            .values_mut()
            .filter(|r| r.binding.member_id == member_id)
        {
            record.revoked = true;
            found = true;
        }
        if !found {
            return Err(AccessError::Rejected);
        }
        Ok(())
    }

    fn active_record(
        &mut self,
        binding: &CredentialBinding,
        now: i64,
    ) -> Result<&StoredCredential, AccessError> {
        self.clock.observe(now)?;
        let record = self
            .records
            .get(&binding.node_id)
            .ok_or(AccessError::Rejected)?;
        if binding.brain_id != self.brain_id
            || &record.binding != binding
            || record.revoked
            || now < record.issued_at
            || now >= record.expires_at
        {
            return Err(AccessError::Rejected);
        }
        Ok(record)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum AccessError {
    #[error("Collaboration access rejected")]
    Rejected,
    #[error("Collaboration access capacity reached")]
    Capacity,
    #[error("Secure random source unavailable")]
    EntropyUnavailable,
}

#[cfg(test)]
#[path = "credential_tests.rs"]
mod tests;
