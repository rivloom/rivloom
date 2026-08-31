use super::*;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;
use std::sync::Mutex;

const NOW: i64 = 1_788_000_000;

#[derive(Default)]
struct Memory {
    entries: Mutex<BTreeMap<String, Zeroizing<Vec<u8>>>>,
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

fn credential() -> IssuedCredential {
    IssuedCredential {
        binding: CredentialBinding {
            brain_id: format!("test-{}", SecretToken::generate().unwrap().expose_secret()),
            member_id: "member".into(),
            node_id: "node".into(),
            device_id: "device".into(),
        },
        expires_at: NOW + 86400,
        secret: SecretToken::generate().unwrap(),
    }
}

#[test]
fn slot_binding_expiry_and_existing_secret_are_preserved() {
    let store = NodeSecrets::new(Memory::default());
    let original = credential();
    assert_eq!(
        store.load(&original.binding, NOW).err(),
        Some(VaultError::Missing)
    );
    store.save_new(&original, NOW).unwrap();
    let restored = store.load(&original.binding, NOW).unwrap();
    assert_eq!(
        (
            &restored.binding,
            restored.expires_at,
            restored.secret.expose_secret()
        ),
        (
            &original.binding,
            original.expires_at,
            original.secret.expose_secret()
        )
    );
    let different = IssuedCredential {
        binding: original.binding.clone(),
        expires_at: original.expires_at,
        secret: SecretToken::generate().unwrap(),
    };
    assert_eq!(store.save_new(&different, NOW), Err(VaultError::Existing));
    assert_eq!(
        store.load(&original.binding, original.expires_at).err(),
        Some(VaultError::Invalid)
    );
    assert_eq!(
        store
            .load(&original.binding, NOW)
            .unwrap()
            .secret
            .expose_secret(),
        original.secret.expose_secret()
    );
    store.remove(&original.binding).unwrap();
    assert_eq!(
        store.load(&original.binding, NOW).err(),
        Some(VaultError::Missing)
    );
}

#[test]
fn case_sensitive_bindings_have_distinct_windows_slots() {
    let original = credential();
    let mut other = original.binding.clone();
    other.node_id = "NODE".into();
    assert_ne!(
        target(&original.binding).unwrap().to_lowercase(),
        target(&other).unwrap().to_lowercase()
    );
}

#[test]
fn invalid_oversized_future_or_misbound_payloads_do_not_delete_the_slot() {
    let store = NodeSecrets::new(Memory::default());
    let original = credential();
    store.save_new(&original, NOW).unwrap();
    let slot = target(&original.binding).unwrap();
    let saved = store.backend.read(&slot).unwrap().unwrap();
    let mut future: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    future["version"] = serde_json::json!(2);
    let mut foreign: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    foreign["binding"]["deviceId"] = serde_json::json!("different");
    for bad in [
        b"{".to_vec(),
        vec![b' '; MAX_SECRET_BYTES + 1],
        serde_json::to_vec(&future).unwrap(),
        serde_json::to_vec(&foreign).unwrap(),
    ] {
        store
            .backend
            .entries
            .lock()
            .unwrap()
            .insert(slot.clone(), Zeroizing::new(bad.clone()));
        assert_eq!(
            store.load(&original.binding, NOW).err(),
            Some(VaultError::Invalid)
        );
        assert_eq!(store.backend.read(&slot).unwrap().unwrap().as_slice(), bad);
    }
}

#[test]
fn secret_fields_are_validated_and_debug_is_redacted() {
    let secret = SecretToken::generate().unwrap();
    let field = SecretField(SecretToken::parse(secret.expose_secret()).unwrap());
    let bytes = serde_json::to_vec(&field).unwrap();
    let decoded: SecretField = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded.0.expose_secret(), secret.expose_secret());
    assert!(!format!("{decoded:?}").contains(secret.expose_secret()));
    assert!(serde_json::from_str::<SecretField>("\"private-invalid-value\"").is_err());
}

#[cfg(windows)]
#[test]
fn windows_vault_round_trip_uses_only_a_fresh_synthetic_slot_and_cleans_up() {
    let original = credential();
    let store = NodeSecrets::new(NativeVault);
    assert_eq!(
        store.load(&original.binding, NOW).err(),
        Some(VaultError::Missing)
    );
    store.save_new(&original, NOW).unwrap();
    struct Cleanup(CredentialBinding);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = NodeSecrets::new(NativeVault).remove(&self.0);
        }
    }
    let cleanup = Cleanup(original.binding.clone());
    let restored = store.load(&original.binding, NOW).unwrap();
    assert_eq!(
        (
            &restored.binding,
            restored.expires_at,
            restored.secret.expose_secret()
        ),
        (
            &original.binding,
            original.expires_at,
            original.secret.expose_secret()
        )
    );
    assert_eq!(store.save_new(&original, NOW), Err(VaultError::Existing));
    drop(cleanup);
    assert_eq!(
        store.load(&original.binding, NOW).err(),
        Some(VaultError::Missing)
    );
}
