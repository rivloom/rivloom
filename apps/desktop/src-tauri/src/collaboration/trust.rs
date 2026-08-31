use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;

use super::protocol::id;
use super::tls::{Peer, private_address};

const MAX_DESCRIPTOR_BYTES: usize = 8192;

/// Public, untrusted bootstrap material. Possession/import does not authorize joining or execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TrustDescriptor {
    version: u32,
    brain_id: String,
    address: SocketAddr,
    server_name: String,
    certificate_der: Vec<u8>,
}

impl TrustDescriptor {
    pub(super) fn new(
        brain_id: String,
        address: SocketAddr,
        server_name: String,
        certificate_der: Vec<u8>,
    ) -> Result<Self, TrustError> {
        let descriptor = Self {
            version: 1,
            brain_id,
            address,
            server_name,
            certificate_der,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self, TrustError> {
        if bytes.len() > MAX_DESCRIPTOR_BYTES {
            return Err(TrustError::Invalid);
        }
        let descriptor: Self = serde_json::from_slice(bytes).map_err(|_| TrustError::Invalid)?;
        descriptor.validate()?;
        Ok(descriptor)
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, TrustError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| TrustError::Invalid)?;
        if bytes.len() > MAX_DESCRIPTOR_BYTES {
            return Err(TrustError::Invalid);
        }
        Ok(bytes)
    }

    pub(super) fn brain_id(&self) -> &str {
        &self.brain_id
    }
    pub(super) fn address(&self) -> SocketAddr {
        self.address
    }
    pub(super) fn server_name(&self) -> &str {
        &self.server_name
    }
    pub(super) fn certificate_der(&self) -> &[u8] {
        &self.certificate_der
    }

    /// Compare this certificate fingerprint through a separate trusted channel before confirming.
    pub(super) fn fingerprint(&self) -> String {
        format!("{:x}", Sha256::digest(&self.certificate_der))
    }

    fn validate(&self) -> Result<(), TrustError> {
        if self.version != 1
            || !id(&self.brain_id)
            || self.address.port() == 0
            || !private_address(self.address.ip())
            || self.server_name.len() > 253
            || self.certificate_der.is_empty()
            || self.certificate_der.len() > 1024
        {
            return Err(TrustError::Invalid);
        }
        self.peer().map(|_| ())
    }

    fn peer(&self) -> Result<Peer, TrustError> {
        Peer::new(
            self.address,
            self.server_name.clone(),
            self.certificate_der.clone(),
            Sha256::digest(&self.certificate_der).into(),
        )
        .map_err(|_| TrustError::Invalid)
    }
}

/// Only explicit out-of-band fingerprint confirmation produces a usable peer; no TOFU or discovery.
pub(super) struct TrustedPeer {
    descriptor: TrustDescriptor,
}

impl TrustedPeer {
    pub(super) fn confirm(bytes: &[u8], confirmed_fingerprint: &str) -> Result<Self, TrustError> {
        let descriptor = TrustDescriptor::decode(bytes)?;
        if confirmed_fingerprint != descriptor.fingerprint() {
            return Err(TrustError::Unconfirmed);
        }
        Ok(Self { descriptor })
    }

    pub(super) fn descriptor(&self) -> &TrustDescriptor {
        &self.descriptor
    }
    pub(super) fn peer(&self) -> Result<Peer, TrustError> {
        self.descriptor.peer()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum TrustError {
    #[error("Invalid Brain trust descriptor")]
    Invalid,
    #[error("Brain certificate fingerprint has not been independently confirmed")]
    Unconfirmed,
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
