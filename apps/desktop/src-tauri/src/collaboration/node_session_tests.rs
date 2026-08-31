use super::super::secret_store::VaultError;
use super::super::test_support::{Fixture, Memory, fixture};
use super::*;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use tempfile::TempDir;
use zeroize::Zeroizing;

fn bob() -> RivloomIdentity {
    RivloomIdentity {
        identity_id: "bob".into(),
        device_id: "bob-device".into(),
        display_name: "Bob".into(),
        brain_membership: None,
    }
}
fn registration(f: &Fixture) -> NodeRegistration {
    NodeRegistration::confirmed(
        &bob(),
        &f.descriptor.encode().unwrap(),
        &f.descriptor.fingerprint(),
    )
    .unwrap()
}
fn owner(f: &Fixture) -> Client {
    let vault = NodeSecrets::new(Memory::default());
    vault.save_new(&f.owner, f.now).unwrap();
    Client::connect(&f.peer, &vault, f.owner.binding.clone()).unwrap()
}

#[test]
fn explicit_join_disconnect_and_restart_keep_the_same_binding_without_reenrollment() {
    let f = fixture();
    let mut alice = owner(&f);
    let invite = alice.invite().unwrap();
    let temp = TempDir::new().unwrap();
    let vault = Arc::new(Memory::default());
    let session = NodeSession::new(temp.path().join("node-client"), vault.clone()).unwrap();
    assert_eq!(
        session.status(&bob()),
        Ok(NodeStatus {
            state: ConnectionState::NotConfigured,
            registration: None,
            binding: None,
            revision: 0
        })
    );
    let registration = registration(&f);
    let joined = session
        .join(&bob(), &registration, &invite.invitation_id, &invite.secret)
        .unwrap();
    assert_eq!(joined.state, ConnectionState::Connected);
    assert_eq!(
        session.join(&bob(), &registration, &invite.invitation_id, &invite.secret),
        Err(SessionError::Existing)
    );
    session.disconnect().unwrap();
    assert_eq!(
        session.status(&bob()),
        Ok(NodeStatus {
            state: ConnectionState::Disconnected,
            ..joined.clone()
        })
    );
    drop(session);
    let reopened = NodeSession::new(temp.path().join("node-client"), vault).unwrap();
    assert_eq!(
        reopened.status(&bob()),
        Ok(NodeStatus {
            state: ConnectionState::Disconnected,
            revision: 0,
            ..joined.clone()
        })
    );
    let connected = reopened.connect(&bob()).unwrap();
    assert_eq!(
        (connected.state, connected.registration, connected.binding),
        (
            ConnectionState::Connected,
            joined.registration,
            joined.binding
        )
    );
    assert!(reopened.refresh(&bob()).unwrap().revision >= joined.revision);
}

#[test]
fn revoked_members_cannot_refresh_or_reconnect_and_their_registration_is_retained() {
    let f = fixture();
    let mut alice = owner(&f);
    let invite = alice.invite().unwrap();
    let temp = TempDir::new().unwrap();
    let session = NodeSession::new(temp.path().join("node-client"), Memory::default()).unwrap();
    let joined = session
        .join(
            &bob(),
            &registration(&f),
            &invite.invitation_id,
            &invite.secret,
        )
        .unwrap();
    alice.reconcile().unwrap();
    alice
        .revoke(joined.binding.as_ref().unwrap().member_id.clone())
        .unwrap();
    assert_eq!(session.refresh(&bob()), Err(SessionError::Rejected));
    assert_eq!(session.connect(&bob()), Err(SessionError::Rejected));
    assert_eq!(
        session.status(&bob()),
        Ok(NodeStatus {
            state: ConnectionState::Disconnected,
            ..joined
        })
    );
}

#[test]
fn failed_transport_keeps_a_durable_attempt_and_never_resends_join_on_restart() {
    let mut f = fixture();
    let invite = owner(&f).invite().unwrap();
    let registration = registration(&f);
    f.server.stop();
    let temp = TempDir::new().unwrap();
    let session = NodeSession::new(temp.path().join("node-client"), Memory::default()).unwrap();
    assert_eq!(
        session.join(&bob(), &registration, &invite.invitation_id, &invite.secret),
        Err(SessionError::Transport)
    );
    drop(session);
    let reopened = NodeSession::new(temp.path().join("node-client"), Memory::default()).unwrap();
    assert_eq!(
        reopened.status(&bob()),
        Ok(NodeStatus {
            state: ConnectionState::RecoveryRequired,
            registration: Some(registration.clone()),
            binding: None,
            revision: 0
        })
    );
    assert_eq!(
        reopened.connect(&bob()),
        Err(SessionError::RecoveryRequired)
    );
    assert_eq!(
        reopened.join(&bob(), &registration, &invite.invitation_id, &invite.secret),
        Err(SessionError::Existing)
    );
}

struct FailedWrite;
impl SecretBackend for FailedWrite {
    fn read(&self, _: &str) -> Result<Option<Zeroizing<Vec<u8>>>, VaultError> {
        Ok(None)
    }
    fn write_new(&self, _: &str, _: &[u8]) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
    fn remove(&self, _: &str) -> Result<(), VaultError> {
        unreachable!()
    }
}

#[test]
fn vault_failure_after_remote_enrollment_does_not_publish_a_connected_local_registration() {
    let f = fixture();
    let mut alice = owner(&f);
    let invite = alice.invite().unwrap();
    let temp = TempDir::new().unwrap();
    let session = NodeSession::new(temp.path().join("node-client"), FailedWrite).unwrap();
    assert_eq!(
        session.join(
            &bob(),
            &registration(&f),
            &invite.invitation_id,
            &invite.secret
        ),
        Err(SessionError::Credential)
    );
    assert_eq!(
        session.status(&bob()).unwrap().state,
        ConnectionState::RecoveryRequired
    );
    assert!(!temp.path().join("node-client/binding-v1.json").exists());
    alice.reconcile().unwrap();
    assert!(alice.view().shared_records().any(|record| matches!(&record.data, super::super::reconcile::SharedData::Member { identity_id, .. } if identity_id == "bob")));
}

#[test]
fn invalid_identity_busy_and_shutdown_prevent_network_or_registration_changes() {
    let f = fixture();
    let invite = owner(&f).invite().unwrap();
    let temp = TempDir::new().unwrap();
    let session = NodeSession::new(temp.path().join("node-client"), Memory::default()).unwrap();
    let foreign = RivloomIdentity {
        device_id: "other".into(),
        ..bob()
    };
    assert_eq!(
        session.join(
            &foreign,
            &registration(&f),
            &invite.invitation_id,
            &invite.secret
        ),
        Err(SessionError::Invalid)
    );
    assert_eq!(
        session.join(&bob(), &registration(&f), "bad", &invite.secret),
        Err(SessionError::Invalid)
    );
    assert!(!temp.path().join("node-client").exists());
    let guard = session.state.lock().unwrap();
    assert_eq!(session.connect(&bob()), Err(SessionError::Busy));
    drop(guard);
    session.shutdown();
    assert_eq!(session.connect(&bob()), Err(SessionError::Unavailable));
    assert_eq!(
        session.join(
            &bob(),
            &registration(&f),
            &invite.invitation_id,
            &invite.secret
        ),
        Err(SessionError::Unavailable)
    );
}
