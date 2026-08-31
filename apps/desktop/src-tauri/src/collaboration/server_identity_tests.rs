use super::super::secret_store::NativeVault;
use super::super::test_support::Memory;
use super::super::tls::TlsChannel;
use super::super::trust::TrustedPeer;
use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::net::TcpListener;

fn address() -> SocketAddr {
    "127.0.0.1:7443".parse().unwrap()
}
fn now() -> i64 {
    super::super::server::now().unwrap()
}

#[test]
fn provision_restore_and_endpoint_change_keep_the_same_pinned_key() {
    let store = ServerIdentityStore::new(Memory::default());
    let time = now();
    let identity = store
        .create("Brain-A", address(), "localhost", time)
        .unwrap();
    let restored = store.load("Brain-A", address(), time + 1).unwrap();
    assert_eq!(restored.descriptor, identity.descriptor);
    assert_eq!(restored.expires_at, time + LIFETIME);
    let changed = store
        .load("Brain-A", "100.64.0.1:7444".parse().unwrap(), time + 1)
        .unwrap();
    assert_eq!(
        changed.descriptor.fingerprint(),
        identity.descriptor.fingerprint()
    );
    assert_ne!(changed.descriptor.address(), identity.descriptor.address());
    assert_eq!(
        store
            .create("Brain-A", address(), "replacement", time)
            .err(),
        Some(VaultError::Existing)
    );
    assert_eq!(
        store.load("brain-a", address(), time).err(),
        Some(VaultError::Missing)
    );
    let another = store
        .create("brain-a", address(), "localhost", time)
        .unwrap();
    assert_ne!(
        another.descriptor.fingerprint(),
        identity.descriptor.fingerprint()
    );
}

#[test]
fn expiry_invalid_clock_and_unsafe_address_fail_without_replacing_the_slot() {
    let store = ServerIdentityStore::new(Memory::default());
    let time = now();
    let identity = store
        .create("brain-1", address(), "localhost", time)
        .unwrap();
    for invalid_time in [time - 1, time + LIFETIME, i64::MAX, -1] {
        assert_eq!(
            store.load("brain-1", address(), invalid_time).err(),
            Some(VaultError::Invalid)
        );
    }
    for address in ["0.0.0.0:7443", "8.8.8.8:7443", "127.0.0.1:0"] {
        assert_eq!(
            store.load("brain-1", address.parse().unwrap(), time).err(),
            Some(VaultError::Invalid)
        );
    }
    assert_eq!(
        store.load("brain-1", address(), time).unwrap().descriptor,
        identity.descriptor
    );
    assert_eq!(
        store.create("../brain", address(), "localhost", time).err(),
        Some(VaultError::Invalid)
    );
    assert_eq!(
        store
            .create("brain-2", address(), "https://localhost", time)
            .err(),
        Some(VaultError::Invalid)
    );
    assert_eq!(
        store.load("brain-2", address(), time).err(),
        Some(VaultError::Missing)
    );
}

#[test]
fn malformed_misbound_and_mismatched_keys_remain_unusable_and_preserved() {
    let store = ServerIdentityStore::new(Memory::default());
    let time = now();
    store
        .create("brain-1", address(), "localhost", time)
        .unwrap();
    let slot = target("brain-1").unwrap();
    let bytes = store.backend.read(&slot).unwrap().unwrap();
    let valid: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let replacement = Zeroizing::new(KeyPair::generate().unwrap());
    for (field, value) in [
        ("version", json!(2)),
        ("brainId", json!("another")),
        ("serverName", json!("invalid name")),
        ("serverName", json!("different.local")),
        ("expiresAt", json!(time + LIFETIME + 1)),
        ("issuedAt", json!(time + 1)),
        ("certificate", json!("not-base64")),
        ("key", json!("not-base64")),
        ("key", json!(STANDARD.encode(vec![0; 257]))),
        ("key", json!(STANDARD.encode(replacement.serialize_der()))),
        ("extra", json!(true)),
    ] {
        let mut bad = valid.clone();
        bad[field] = value;
        let encoded = Zeroizing::new(serde_json::to_vec(&bad).unwrap());
        let hash = Sha256::digest(&encoded);
        store
            .backend
            .entries
            .lock()
            .unwrap()
            .insert(slot.clone(), encoded);
        assert_eq!(
            store.load("brain-1", address(), time).err(),
            Some(VaultError::Invalid),
            "{field}"
        );
        assert_eq!(
            Sha256::digest(&*store.backend.read(&slot).unwrap().unwrap()),
            hash
        );
    }
    for bad in [b"broken".to_vec(), vec![b' '; MAX_VAULT_BYTES + 1]] {
        store
            .backend
            .entries
            .lock()
            .unwrap()
            .insert(slot.clone(), Zeroizing::new(bad));
        assert_eq!(
            store.load("brain-1", address(), time).err(),
            Some(VaultError::Invalid)
        );
        assert_eq!(
            store.create("brain-1", address(), "localhost", time).err(),
            Some(VaultError::Existing)
        );
    }
}

#[test]
fn restored_identity_completes_real_tls_with_independently_confirmed_public_material() {
    let store = ServerIdentityStore::new(Memory::default());
    let time = now();
    store
        .create("brain-1", address(), "localhost", time)
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let identity = store
        .load("brain-1", listener.local_addr().unwrap(), time)
        .unwrap();
    let trusted = TrustedPeer::confirm(
        &identity.descriptor.encode().unwrap(),
        &identity.descriptor.fingerprint(),
    )
    .unwrap();
    let worker = std::thread::spawn(move || {
        let mut channel = TlsChannel::accept(listener.accept().unwrap().0, &identity.tls).unwrap();
        assert_eq!(channel.receive().unwrap().as_slice(), b"hello");
        channel.send(b"restored").unwrap();
    });
    let mut channel = TlsChannel::connect(&trusted.peer().unwrap()).unwrap();
    channel.send(b"hello").unwrap();
    assert_eq!(channel.receive().unwrap().as_slice(), b"restored");
    worker.join().unwrap();
}

#[cfg(windows)]
#[test]
fn native_vault_protects_a_new_synthetic_tls_identity_and_preserves_namespace_boundaries() {
    let brain_id = format!(
        "test-{}",
        super::super::credential::SecretToken::generate()
            .unwrap()
            .expose_secret()
    );
    let store = ServerIdentityStore::new(NativeVault);
    let slot = target(&brain_id).unwrap();
    assert!(store.backend.read(&slot).unwrap().is_none());
    let identity = store
        .create(&brain_id, address(), "localhost", now())
        .unwrap();
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = NativeVault.remove(&self.0);
        }
    }
    let cleanup = Cleanup(slot.clone());
    assert_eq!(
        store.load(&brain_id, address(), now()).unwrap().descriptor,
        identity.descriptor
    );
    assert!(store.backend.read(&slot).unwrap().unwrap().len() <= MAX_VAULT_BYTES);
    for target in [
        "Rivloom/other/v1/abc",
        "Microsoft/test",
        "Rivloom/brain-tls/v1/../",
    ] {
        assert_eq!(store.backend.read(target).err(), Some(VaultError::Invalid));
        assert_eq!(
            store.backend.write_new(target, b"synthetic"),
            Err(VaultError::Invalid)
        );
    }
    assert_eq!(
        store
            .backend
            .write_new(&slot, &vec![0; MAX_VAULT_BYTES + 1]),
        Err(VaultError::Invalid)
    );
    drop(cleanup);
    assert!(store.backend.read(&slot).unwrap().is_none());
}
