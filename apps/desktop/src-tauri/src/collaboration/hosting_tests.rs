use super::super::client::Client;
use super::super::secret_store::VaultError;
use super::super::test_support::Memory;
use super::super::trust::TrustedPeer;
use super::*;
use pretty_assertions::assert_eq;
use std::net::{TcpListener, TcpStream};
use tempfile::TempDir;
use zeroize::Zeroizing;

fn owner() -> RivloomIdentity {
    RivloomIdentity {
        identity_id: "alice".into(),
        device_id: "alice-device".into(),
        display_name: "Alice".into(),
        brain_membership: None,
    }
}
fn setup() -> (TempDir, BrainService<Memory>, TcpListener) {
    let temp = TempDir::new().unwrap();
    let service = BrainService::new(temp.path().join("brain-host"), Memory::default()).unwrap();
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    (temp, service, reservation)
}

#[test]
fn provisioning_is_explicit_and_does_not_start_a_listener_or_overwrite_a_profile() {
    let (_temp, service, reservation) = setup();
    assert_eq!(service.status(), Ok(HostingStatus::NotConfigured));
    assert!(!service.directory.exists());
    let address = reservation.local_addr().unwrap();
    let profile = service.initialize(&owner(), address, "localhost").unwrap();
    assert_eq!(
        service.status(),
        Ok(HostingStatus::Stopped(profile.clone()))
    );
    assert_eq!(
        service.initialize(&owner(), address, "localhost"),
        Err(HostingError::Existing)
    );
    assert_eq!(HostProfile::load(&service.directory).unwrap(), profile);
    let file = fs::read_to_string(service.directory.join("host-v1.json")).unwrap();
    for forbidden in ["secret", "key", "verifier", "codex-home"] {
        assert!(!file.contains(forbidden));
    }
    drop(reservation);
    assert!(TcpListener::bind(address).is_ok());
}

#[test]
fn start_stop_and_restart_use_the_same_registered_identity_over_real_tls() {
    let (temp, service, reservation) = setup();
    let profile = service
        .initialize(&owner(), reservation.local_addr().unwrap(), "localhost")
        .unwrap();
    let trusted = TrustedPeer::confirm(
        &profile.descriptor.encode().unwrap(),
        &profile.descriptor.fingerprint(),
    )
    .unwrap();
    let backend = service.backend.clone();
    drop(reservation);
    assert_eq!(
        service.start(&owner()),
        Ok(HostingStatus::Running(profile.clone()))
    );
    assert_eq!(service.start(&owner()), Err(HostingError::Busy));
    let mut client = Client::connect(
        &trusted.peer().unwrap(),
        &NodeSecrets::new(backend.clone()),
        profile.binding.clone(),
    )
    .unwrap();
    client.pulse().unwrap();
    service.stop().unwrap();
    assert!(client.pulse().is_err());
    assert_eq!(
        service.status(),
        Ok(HostingStatus::Stopped(profile.clone()))
    );
    assert!(TcpStream::connect(profile.descriptor.address()).is_err());
    service.stop().unwrap();
    drop(service);
    let reopened = BrainService::new(temp.path().join("brain-host"), backend).unwrap();
    assert_eq!(
        reopened.start(&owner()),
        Ok(HostingStatus::Running(profile))
    );
    let client = Client::connect(
        &trusted.peer().unwrap(),
        &NodeSecrets::new(reopened.backend.clone()),
        client.binding().clone(),
    )
    .unwrap();
    assert!(client.view().is_ready());
    reopened.shutdown();
    assert_eq!(reopened.start(&owner()), Err(HostingError::Unavailable));
}

#[test]
fn bind_failure_and_wrong_local_identity_never_report_running() {
    let (_temp, service, reservation) = setup();
    let profile = service
        .initialize(&owner(), reservation.local_addr().unwrap(), "localhost")
        .unwrap();
    assert_eq!(service.start(&owner()), Err(HostingError::Unavailable));
    assert_eq!(
        service.status(),
        Ok(HostingStatus::Stopped(profile.clone()))
    );
    let mut wrong = owner();
    wrong.identity_id = "mallory".into();
    assert_eq!(service.start(&wrong), Err(HostingError::Invalid));
    wrong = owner();
    wrong.device_id = "another-device".into();
    assert_eq!(service.start(&wrong), Err(HostingError::Invalid));
    drop(reservation);
    assert_eq!(service.start(&owner()), Ok(HostingStatus::Running(profile)));
    service.shutdown();
}

struct FailedVault;
impl SecretBackend for FailedVault {
    fn read(&self, _target: &str) -> Result<Option<Zeroizing<Vec<u8>>>, VaultError> {
        Ok(None)
    }
    fn write_new(&self, _target: &str, _bytes: &[u8]) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
    fn remove(&self, _target: &str) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
}

#[test]
fn partial_provisioning_remains_visible_and_cannot_be_silently_reinitialized() {
    let temp = TempDir::new().unwrap();
    let service = BrainService::new(temp.path().join("brain-host"), FailedVault).unwrap();
    let address = "127.0.0.1:7443".parse().unwrap();
    assert_eq!(
        service.initialize(&owner(), address, "localhost"),
        Err(HostingError::Credential)
    );
    assert_eq!(service.status(), Err(HostingError::Incomplete));
    assert_eq!(service.start(&owner()), Err(HostingError::Incomplete));
    let before = fs::read(service.directory.join("brain-v1.json")).unwrap();
    assert_eq!(
        service.initialize(&owner(), address, "localhost"),
        Err(HostingError::Existing)
    );
    assert_eq!(
        fs::read(service.directory.join("brain-v1.json")).unwrap(),
        before
    );
}

#[test]
fn corrupt_registration_and_missing_credentials_are_not_reset_or_replaced() {
    let (_temp, service, reservation) = setup();
    let profile = service
        .initialize(&owner(), reservation.local_addr().unwrap(), "localhost")
        .unwrap();
    let path = service.directory.join("host-v1.json");
    let valid = serde_json::to_vec(&profile).unwrap();
    for bytes in [
        b"broken".to_vec(),
        vec![b' '; 12289],
        String::from_utf8(valid.clone())
            .unwrap()
            .replacen("{", "{\"version\":1,", 1)
            .into_bytes(),
    ] {
        fs::write(&path, &bytes).unwrap();
        assert_eq!(service.status(), Err(HostingError::Invalid));
        assert_eq!(service.start(&owner()), Err(HostingError::Invalid));
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::write(&path, &valid).unwrap();
    NodeSecrets::new(service.backend.clone())
        .remove(&profile.binding)
        .unwrap();
    assert_eq!(service.start(&owner()), Err(HostingError::Credential));
    assert_eq!(fs::read(&path).unwrap(), valid);
}

#[test]
fn invalid_inputs_and_busy_or_closed_service_cannot_begin_provisioning() {
    assert!(matches!(
        BrainService::new("relative".into(), Memory::default()),
        Err(HostingError::Invalid)
    ));
    let (_temp, service, _reservation) = setup();
    for address in ["0.0.0.0:7443", "8.8.8.8:7443", "127.0.0.1:0"] {
        assert_eq!(
            service.initialize(&owner(), address.parse().unwrap(), "localhost"),
            Err(HostingError::Invalid)
        );
        assert!(!service.directory.exists());
    }
    let held = service.state.lock().unwrap();
    assert_eq!(service.start(&owner()), Err(HostingError::Busy));
    assert_eq!(service.stop(), Err(HostingError::Busy));
    drop(held);
    service.shutdown();
    assert_eq!(
        service.initialize(&owner(), "127.0.0.1:7443".parse().unwrap(), "localhost"),
        Err(HostingError::Unavailable)
    );
    assert!(!service.directory.exists());
}
