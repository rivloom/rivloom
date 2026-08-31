use super::*;
use pretty_assertions::assert_eq;
use rcgen::{CertificateParams, KeyPair};
use serde_json::json;
use tempfile::TempDir;

fn identity() -> RivloomIdentity {
    RivloomIdentity { identity_id: "bob".into(), device_id: "bob-device".into(), display_name: "Bob".into(), brain_membership: None }
}

fn registration() -> NodeRegistration {
    let key = KeyPair::generate().unwrap();
    let cert = CertificateParams::new(vec!["localhost".into()]).unwrap().self_signed(&key).unwrap();
    let descriptor = TrustDescriptor::new("brain-1".into(), "127.0.0.1:7443".parse().unwrap(), "localhost".into(), cert.der().to_vec()).unwrap();
    // The fixture knows the generated server certificate independently of the imported descriptor.
    let fingerprint = descriptor.fingerprint();
    NodeRegistration::confirmed(&identity(), &descriptor.encode().unwrap(), &fingerprint).unwrap()
}

fn binding() -> CredentialBinding {
    CredentialBinding { brain_id: "brain-1".into(), member_id: "member-bob".into(), node_id: "node-bob".into(), device_id: "bob-device".into() }
}

#[test]
fn durable_attempt_precedes_binding_and_neither_can_be_overwritten_on_restart() {
    let temp = TempDir::new().unwrap();
    let store = RegistrationStore::new(temp.path().join("node-client")).unwrap();
    let registration = registration();
    assert_eq!(store.load(&identity()), Err(RegistrationError::NotConfigured));
    assert!(!store.directory.exists());
    store.begin(&registration).unwrap();
    assert_eq!(store.load(&identity()), Ok(registration.clone()));
    assert_eq!(store.binding(&registration), Err(RegistrationError::Incomplete));
    let reopened = RegistrationStore::new(store.directory.clone()).unwrap();
    assert_eq!(reopened.begin(&registration), Err(RegistrationError::Existing));
    reopened.complete(&registration, &binding()).unwrap();
    assert_eq!(reopened.binding(&registration), Ok(binding()));
    assert_eq!(reopened.complete(&registration, &binding()), Err(RegistrationError::Existing));
    let disk: serde_json::Value = serde_json::from_slice(&fs::read(store.directory.join("registration-v1.json")).unwrap()).unwrap();
    assert_eq!(disk, json!({"version":1,"identityId":"bob","deviceId":"bob-device","descriptor":registration.descriptor,"confirmedFingerprint":registration.confirmed_fingerprint}));
}

#[test]
fn identity_binding_and_prior_fingerprint_must_match_without_implicit_reconfirmation() {
    let temp = TempDir::new().unwrap();
    let store = RegistrationStore::new(temp.path().join("node-client")).unwrap();
    let registration = registration();
    store.begin(&registration).unwrap();
    for foreign in [RivloomIdentity { identity_id: "other".into(), ..identity() }, RivloomIdentity { device_id: "other".into(), ..identity() }] {
        assert_eq!(store.load(&foreign), Err(RegistrationError::Invalid));
    }
    for foreign in [CredentialBinding { brain_id: "other".into(), ..binding() }, CredentialBinding { device_id: "other".into(), ..binding() }] {
        assert_eq!(store.complete(&registration, &foreign), Err(RegistrationError::Invalid));
    }
    assert!(!store.directory.join("binding-v1.json").exists());
    assert!(matches!(NodeRegistration::confirmed(&identity(), &registration.descriptor.encode().unwrap(), &"0".repeat(64)), Err(RegistrationError::Invalid)));
    let mut changed = registration.clone();
    changed.confirmed_fingerprint = "0".repeat(64);
    fs::write(store.directory.join("registration-v1.json"), serde_json::to_vec(&changed).unwrap()).unwrap();
    assert_eq!(store.load(&identity()), Err(RegistrationError::Invalid));
    assert_eq!(store.complete(&registration, &binding()), Err(RegistrationError::Invalid));
}

#[test]
fn malformed_oversized_or_incomplete_records_remain_available_for_explicit_recovery() {
    let temp = TempDir::new().unwrap();
    let store = RegistrationStore::new(temp.path().join("node-client")).unwrap();
    fs::create_dir(&store.directory).unwrap();
    let registration = registration();
    assert_eq!(store.load(&identity()), Err(RegistrationError::Incomplete));
    assert_eq!(store.begin(&registration), Err(RegistrationError::Existing));
    let path = store.directory.join("registration-v1.json");
    let mut unknown = serde_json::to_value(&registration).unwrap();
    unknown["secret"] = json!("never-accepted");
    for bytes in [vec![b'x'; 12 * 1024 + 1], b"{broken".to_vec(), serde_json::to_vec(&unknown).unwrap(), b"{\"version\":1,\"version\":1}".to_vec()] {
        fs::write(&path, &bytes).unwrap();
        assert_eq!(store.load(&identity()), Err(RegistrationError::Invalid));
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::write(&path, serde_json::to_vec(&registration).unwrap()).unwrap();
    fs::write(store.directory.join("binding-v1.json"), vec![b'x'; 1025]).unwrap();
    assert_eq!(store.binding(&registration), Err(RegistrationError::Invalid));
}

#[test]
fn invalid_paths_or_descriptors_do_not_create_enrollment_storage() {
    assert!(matches!(RegistrationStore::new("relative/node".into()), Err(RegistrationError::Invalid)));
    let temp = TempDir::new().unwrap();
    let store = RegistrationStore::new(temp.path().join("node-client")).unwrap();
    let mut invalid = registration();
    invalid.version = 2;
    assert_eq!(store.begin(&invalid), Err(RegistrationError::Invalid));
    assert!(!store.directory.exists());
    fs::write(&store.directory, "occupied").unwrap();
    assert_eq!(store.begin(&registration()), Err(RegistrationError::Existing));
    assert_eq!(store.load(&identity()), Err(RegistrationError::Invalid));
}
