use super::super::reconcile::{MAX_CONTROL_BYTES, ReconcileRequest};
use super::super::secret_store::SecretField;
use super::super::test_support::{authenticated, fixture, rpc};
use super::super::wire::{Operation, Reply};
use super::*;
use pretty_assertions::assert_eq;

#[test]
fn native_tls_listener_requires_authentication_before_state_access() {
    let f = fixture();
    let mut fresh = TlsChannel::connect(&f.peer).unwrap();
    assert!(matches!(
        rpc(
            &mut fresh,
            Operation::Sync(ReconcileRequest {
                after: 0,
                at: None,
                offset: 0
            })
        ),
        Reply::Error(WireError::Rejected)
    ));
    assert!(fresh.receive().is_err());
    let mut owner = authenticated(&f);
    assert!(matches!(
        rpc(&mut owner, Operation::Pulse {}),
        Reply::Pulsed { revision: 2 }
    ));
    assert!(matches!(
        rpc(
            &mut owner,
            Operation::Authenticate {
                binding: f.owner.binding.clone(),
                secret: SecretField(
                    super::super::credential::SecretToken::parse(f.owner.secret.expose_secret())
                        .unwrap()
                ),
            }
        ),
        Reply::Error(WireError::Rejected)
    ));
}

#[test]
fn invitation_join_and_member_authentication_run_through_real_tls() {
    let f = fixture();
    let mut alice = authenticated(&f);
    let Reply::Invited {
        brain_id,
        invitation_id,
        secret,
        ..
    } = rpc(&mut alice, Operation::Invite {})
    else {
        panic!("invite");
    };
    let mut bob = TlsChannel::connect(&f.peer).unwrap();
    let Reply::Joined {
        binding,
        secret: node_secret,
        ..
    } = rpc(
        &mut bob,
        Operation::Join {
            brain_id,
            invitation_id,
            secret,
            identity_id: "bob".into(),
            device_id: "bob-device".into(),
            display_name: "Bob".into(),
        },
    )
    else {
        panic!("join");
    };
    assert_eq!(binding.device_id, "bob-device");
    assert!(matches!(
        rpc(
            &mut bob,
            Operation::Authenticate {
                binding: binding.clone(),
                secret: node_secret
            }
        ),
        Reply::Authenticated {}
    ));
    let Reply::Page(page) = rpc(
        &mut bob,
        Operation::Sync(ReconcileRequest {
            after: 0,
            at: None,
            offset: 0,
        }),
    ) else {
        panic!("sync");
    };
    assert_eq!(page.member_id, binding.member_id);
    let text = String::from_utf8(page.encode().unwrap()).unwrap();
    for forbidden in [
        "verifier",
        "credentials",
        "invitations",
        f.owner.secret.expose_secret(),
    ] {
        assert!(!text.contains(forbidden));
    }
}

#[test]
fn stopping_the_listener_closes_idle_authenticated_and_incomplete_handshakes() {
    let mut f = fixture();
    let mut active = authenticated(&f);
    let mut raw = TcpStream::connect(f.server.address).unwrap();
    raw.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    f.server.stop();
    assert!(active.receive().is_err());
    use std::io::Read;
    let mut byte = [0; 1];
    assert!(!matches!(raw.read(&mut byte), Ok(1)));
    // Stop is idempotent; dropping the fixture cannot leave a reader or listener alive.
    f.server.stop();
    assert!(TcpStream::connect(f.server.address).is_err());
}

#[test]
fn malformed_maximum_frame_does_not_poison_the_brain_or_another_connection() {
    let f = fixture();
    let mut bad = TlsChannel::connect(&f.peer).unwrap();
    bad.send(&vec![b'x'; MAX_CONTROL_BYTES]).unwrap();
    assert!(bad.receive().is_err());
    let mut owner = authenticated(&f);
    assert!(matches!(
        rpc(&mut owner, Operation::Pulse {}),
        Reply::Pulsed { revision: 2 }
    ));
}

#[test]
fn shutdown_releases_session_permits_for_future_listeners() {
    let mut f = fixture();
    let mut active = authenticated(&f);
    f.server.stop();
    assert!(active.receive().is_err());
    let mut sessions = Vec::new();
    for _ in 0..16 {
        sessions.push(f.host.session().unwrap());
    }
    assert!(f.host.session().is_err());
}
