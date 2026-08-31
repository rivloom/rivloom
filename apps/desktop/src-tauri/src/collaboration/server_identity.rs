use base64::{Engine, engine::general_purpose::STANDARD};
use rcgen::{CertificateParams, ExtendedKeyUsagePurpose, KeyPair};
use rustls::RootCertStore;
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

use super::protocol::{id, timestamp};
use super::secret_store::{MAX_VAULT_BYTES, SERVER_KEY_PREFIX, SecretBackend, VaultError};
use super::tls::ServerTls;
use super::trust::TrustDescriptor;

const LIFETIME: i64 = 30 * 86400;

struct SecretDer(Zeroizing<Vec<u8>>);
impl Serialize for SecretDer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let encoded = Zeroizing::new(STANDARD.encode(&self.0));
        serializer.serialize_str(&encoded)
    }
}
impl<'de> Deserialize<'de> for SecretDer {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let encoded = Zeroizing::new(String::deserialize(deserializer)?);
        if encoded.len() > 344 {
            return Err(serde::de::Error::custom("Invalid protected TLS key"));
        }
        let decoded = Zeroizing::new(
            STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| serde::de::Error::custom("Invalid protected TLS key"))?,
        );
        if decoded.is_empty() || decoded.len() > 256 {
            return Err(serde::de::Error::custom("Invalid protected TLS key"));
        }
        Ok(Self(decoded))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    version: u32,
    brain_id: String,
    server_name: String,
    issued_at: i64,
    expires_at: i64,
    certificate: String,
    key: SecretDer,
}

/// Restored TLS configuration and public bootstrap material; no key serialization surface.
pub(super) struct ServerIdentity {
    pub(super) tls: ServerTls,
    pub(super) descriptor: TrustDescriptor,
    pub(super) expires_at: i64,
}

pub(super) struct ServerIdentityStore<B> {
    backend: B,
}

impl<B: SecretBackend> ServerIdentityStore<B> {
    pub(super) fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Local explicit provisioning only. Never overwrite a key or silently rotate a pinned identity.
    pub(super) fn create(
        &self,
        brain_id: &str,
        address: SocketAddr,
        server_name: &str,
        now: i64,
    ) -> Result<ServerIdentity, VaultError> {
        let slot = target(brain_id)?;
        if !timestamp(now) || server_name.len() > 253 {
            return Err(VaultError::Invalid);
        }
        if self.backend.read(&slot)?.is_some() {
            return Err(VaultError::Existing);
        }
        let expires_at = now
            .checked_add(LIFETIME)
            .filter(|value| timestamp(*value))
            .ok_or(VaultError::Invalid)?;
        let key = Zeroizing::new(KeyPair::generate().map_err(|_| VaultError::Unavailable)?);
        let mut params =
            CertificateParams::new(vec![server_name.into()]).map_err(|_| VaultError::Invalid)?;
        params.not_before = time::OffsetDateTime::from_unix_timestamp(now.saturating_sub(300))
            .map_err(|_| VaultError::Invalid)?;
        params.not_after = time::OffsetDateTime::from_unix_timestamp(expires_at)
            .map_err(|_| VaultError::Invalid)?;
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let certificate = params.self_signed(&*key).map_err(|_| VaultError::Invalid)?;
        let document = Document {
            version: 1,
            brain_id: brain_id.into(),
            server_name: server_name.into(),
            issued_at: now,
            expires_at,
            certificate: STANDARD.encode(certificate.der()),
            key: SecretDer(Zeroizing::new(key.serialize_der())),
        };
        let identity = restore(&document, brain_id, address, now)?;
        let bytes = Zeroizing::new(serde_json::to_vec(&document).map_err(|_| VaultError::Invalid)?);
        if bytes.len() > MAX_VAULT_BYTES {
            return Err(VaultError::Invalid);
        }
        self.backend.write_new(&slot, &bytes)?;
        Ok(identity)
    }

    pub(super) fn load(
        &self,
        brain_id: &str,
        address: SocketAddr,
        now: i64,
    ) -> Result<ServerIdentity, VaultError> {
        let bytes = self
            .backend
            .read(&target(brain_id)?)?
            .ok_or(VaultError::Missing)?;
        if bytes.len() > MAX_VAULT_BYTES {
            return Err(VaultError::Invalid);
        }
        let document: Document = serde_json::from_slice(&bytes).map_err(|_| VaultError::Invalid)?;
        restore(&document, brain_id, address, now)
    }
}

fn restore(
    document: &Document,
    brain_id: &str,
    address: SocketAddr,
    now: i64,
) -> Result<ServerIdentity, VaultError> {
    if document.version != 1
        || document.brain_id != brain_id
        || !timestamp(now)
        || !timestamp(document.issued_at)
        || !timestamp(document.expires_at)
        || document.issued_at.checked_add(LIFETIME) != Some(document.expires_at)
        || now < document.issued_at
        || now >= document.expires_at
        || document.key.0.is_empty()
        || document.key.0.len() > 256
    {
        return Err(VaultError::Invalid);
    }
    let certificate = STANDARD
        .decode(&document.certificate)
        .map_err(|_| VaultError::Invalid)?;
    let descriptor = TrustDescriptor::new(
        brain_id.into(),
        address,
        document.server_name.clone(),
        certificate.clone(),
    )
    .map_err(|_| VaultError::Invalid)?;
    let certificate = CertificateDer::from(certificate);
    let mut roots = RootCertStore::empty();
    roots
        .add(certificate.clone())
        .map_err(|_| VaultError::Invalid)?;
    let verifier = WebPkiServerVerifier::builder_with_provider(
        Arc::new(roots),
        Arc::new(rustls::crypto::ring::default_provider()),
    )
    .build()
    .map_err(|_| VaultError::Invalid)?;
    let name =
        ServerName::try_from(document.server_name.as_str()).map_err(|_| VaultError::Invalid)?;
    verifier
        .verify_server_cert(
            &certificate,
            &[],
            &name,
            &[],
            UnixTime::since_unix_epoch(Duration::from_secs(now as u64)),
        )
        .map_err(|_| VaultError::Invalid)?;
    let tls = ServerTls::new(
        vec![certificate],
        PrivatePkcs8KeyDer::from(document.key.0.to_vec()).into(),
    )
    .map_err(|_| VaultError::Invalid)?;
    Ok(ServerIdentity {
        tls,
        descriptor,
        expires_at: document.expires_at,
    })
}

fn target(brain_id: &str) -> Result<String, VaultError> {
    if !id(brain_id) {
        return Err(VaultError::Invalid);
    }
    Ok(format!(
        "{SERVER_KEY_PREFIX}{:x}",
        Sha256::digest(brain_id.as_bytes())
    ))
}

#[cfg(test)]
#[path = "server_identity_tests.rs"]
mod tests;
