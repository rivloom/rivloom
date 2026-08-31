use rcgen::{CertificateParams, KeyPair};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use zeroize::Zeroizing;

use super::brain::OwnerProfile;
use super::credential::{IssuedCredential, SecretToken};
use super::host::Host;
use super::secret_store::{SecretBackend, SecretField, VaultError};
use super::server::{Server, now};
use super::storage::BrainStore;
use super::tls::{Peer, ServerTls, TlsChannel};
use super::wire::{Operation, Reply, Request, Response};

#[derive(Default)]
pub(super) struct Memory {
    pub(super) entries: Mutex<BTreeMap<String, Zeroizing<Vec<u8>>>>,
}
impl SecretBackend for Memory {
    fn read(&self, target: &str) -> Result<Option<Zeroizing<Vec<u8>>>, VaultError> {
        Ok(self.entries.lock().unwrap().get(target).cloned())
    }
    fn write_new(&self, target: &str, bytes: &[u8]) -> Result<(), VaultError> {
        let mut entries = self.entries.lock().unwrap();
        if entries.contains_key(target) {
            return Err(VaultError::Existing);
        }
        entries.insert(target.into(), Zeroizing::new(bytes.to_vec()));
        Ok(())
    }
    fn remove(&self, target: &str) -> Result<(), VaultError> {
        self.entries.lock().unwrap().remove(target);
        Ok(())
    }
}

// Keep the directory last so the listener and authority lock drop before temporary-file cleanup.
pub(super) struct Fixture {
    pub(super) server: Server,
    pub(super) peer: Peer,
    pub(super) owner: IssuedCredential,
    pub(super) host: Arc<Host>,
    pub(super) now: i64,
    _directory: TempDir,
}

pub(super) fn fixture() -> Fixture {
    let directory = TempDir::new().unwrap();
    let now = now().unwrap();
    let (store, owner) = BrainStore::create(
        directory.path().into(),
        "brain-1".into(),
        OwnerProfile {
            identity_id: "alice",
            device_id: "alice-device",
            display_name: "Alice",
        },
        now,
    )
    .unwrap();
    let host = Host::new(store);
    let key = KeyPair::generate().unwrap();
    let cert = CertificateParams::new(vec!["localhost".into()])
        .unwrap()
        .self_signed(&key)
        .unwrap()
        .der()
        .to_vec();
    let pin = Sha256::digest(&cert).into();
    let tls = ServerTls::new(
        vec![CertificateDer::from(cert.clone())],
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
    )
    .unwrap();
    let server = Server::start(host.clone(), tls, "127.0.0.1:0".parse().unwrap()).unwrap();
    let peer = Peer::new(server.address, "localhost".into(), cert, pin).unwrap();
    Fixture {
        server,
        peer,
        owner,
        host,
        now,
        _directory: directory,
    }
}

pub(super) fn rpc(channel: &mut TlsChannel, operation: Operation) -> Reply {
    let request = Request {
        version: 1,
        id: SecretToken::generate().unwrap().expose_secret().into(),
        operation,
    };
    channel.send(&request.encode().unwrap()).unwrap();
    Response::decode(&channel.receive().unwrap(), &request.id)
        .unwrap()
        .result
}

pub(super) fn authenticated(fixture: &Fixture) -> TlsChannel {
    let mut channel = TlsChannel::connect(&fixture.peer).unwrap();
    assert!(matches!(
        rpc(
            &mut channel,
            Operation::Authenticate {
                binding: fixture.owner.binding.clone(),
                secret: SecretField(
                    SecretToken::parse(fixture.owner.secret.expose_secret()).unwrap()
                ),
            }
        ),
        Reply::Authenticated {}
    ));
    channel
}
