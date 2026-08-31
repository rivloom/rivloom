use super::super::brain::OwnerProfile;
use super::super::credential::{IssuedCredential, SecretToken};
use super::super::reconcile::ReconcileRequest;
use super::super::secret_store::SecretField;
use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;

const NOW: i64 = 1_788_000_000;

fn host() -> (TempDir, Arc<Host>, IssuedCredential) {
    let dir = TempDir::new().unwrap();
    let (store, owner) = BrainStore::create(
        dir.path().into(),
        "brain-1".into(),
        OwnerProfile {
            identity_id: "alice",
            device_id: "alice-device",
            display_name: "Alice",
        },
        NOW,
    )
    .unwrap();
    (dir, Host::new(store), owner)
}
fn call(session: &mut HostSession, operation: Operation, now: i64) -> Reply {
    session
        .handle(
            Request {
                version: 1,
                id: SecretToken::generate().unwrap().expose_secret().into(),
                operation,
            },
            now,
        )
        .result
}
fn authenticate(host: &Arc<Host>, credential: &IssuedCredential) -> HostSession {
    let mut session = host.session().unwrap();
    assert!(matches!(
        call(
            &mut session,
            Operation::Authenticate {
                binding: credential.binding.clone(),
                secret: SecretField(SecretToken::parse(credential.secret.expose_secret()).unwrap()),
            },
            NOW
        ),
        Reply::Authenticated {}
    ));
    session
}
fn join_operation(brain_id: &str, invitation_id: &str, secret: &SecretToken) -> Operation {
    Operation::Join {
        brain_id: brain_id.into(),
        invitation_id: invitation_id.into(),
        secret: SecretField(SecretToken::parse(secret.expose_secret()).unwrap()),
        identity_id: "bob".into(),
        device_id: "bob-device".into(),
        display_name: "Bob".into(),
    }
}

#[test]
fn unauthenticated_and_expired_requests_fail_closed() {
    let (_dir, host, owner) = host();
    let mut fresh = host.session().unwrap();
    assert!(matches!(
        call(
            &mut fresh,
            Operation::Sync(ReconcileRequest {
                after: 0,
                at: None,
                offset: 0
            }),
            NOW
        ),
        Reply::Error(WireError::Rejected)
    ));
    assert!(fresh.closed());
    let mut session = authenticate(&host, &owner);
    assert!(matches!(
        call(&mut session, Operation::Pulse {}, owner.expires_at),
        Reply::Error(WireError::Rejected)
    ));
    assert!(session.closed());
}

#[test]
fn owner_administration_join_once_and_live_revocation_use_committed_authority() {
    let (_dir, host, owner) = host();
    let mut alice = authenticate(&host, &owner);
    let Reply::Invited {
        brain_id,
        invitation_id,
        secret,
        ..
    } = call(&mut alice, Operation::Invite {}, NOW)
    else {
        panic!("invite");
    };
    let mut enrolling = host.session().unwrap();
    let Reply::Joined {
        binding,
        expires_at,
        secret: node_secret,
    } = call(
        &mut enrolling,
        join_operation(&brain_id, &invitation_id, &secret.0),
        NOW,
    )
    else {
        panic!("join");
    };
    let bob = IssuedCredential {
        binding,
        expires_at,
        secret: node_secret.0,
    };
    let mut replay = host.session().unwrap();
    assert!(matches!(
        call(
            &mut replay,
            join_operation(&brain_id, &invitation_id, &secret.0),
            NOW
        ),
        Reply::Error(WireError::Rejected)
    ));
    let mut unauthorized = authenticate(&host, &bob);
    assert!(matches!(
        call(&mut unauthorized, Operation::Invite {}, NOW),
        Reply::Error(WireError::Rejected)
    ));
    let mut active = authenticate(&host, &bob);
    let revision = host.store.lock().unwrap().brain().unwrap().revision();
    assert!(matches!(
        call(
            &mut alice,
            Operation::Revoke {
                member_id: bob.binding.member_id.clone(),
                revision
            },
            NOW
        ),
        Reply::Administered { .. }
    ));
    assert!(matches!(
        call(
            &mut active,
            Operation::Sync(ReconcileRequest {
                after: 0,
                at: None,
                offset: 0
            }),
            NOW
        ),
        Reply::Error(WireError::Rejected)
    ));
    assert!(active.closed());
}

#[test]
fn duplicate_control_id_is_rejected_before_a_second_mutation() {
    let (_dir, host, owner) = host();
    let mut session = authenticate(&host, &owner);
    let request = || Request {
        version: 1,
        id: "pulse".into(),
        operation: Operation::Pulse {},
    };
    assert!(matches!(
        session.handle(request(), NOW).result,
        Reply::Pulsed { revision: 2 }
    ));
    assert!(matches!(
        session.handle(request(), NOW).result,
        Reply::Error(WireError::Rejected)
    ));
    assert_eq!(host.store.lock().unwrap().brain().unwrap().revision(), 2);
}

#[test]
fn concurrent_sessions_and_admission_rate_have_hard_limits() {
    let (_dir, host, _) = host();
    let mut sessions = Vec::new();
    for _ in 0..16 {
        sessions.push(host.session().unwrap());
    }
    assert!(matches!(host.session(), Err(WireError::Busy)));
    drop(sessions.pop());
    assert!(host.session().is_ok());
    let now = Instant::now();
    let mut limiter = Limiter::new(/*limit*/ 2, now);
    assert!(limiter.take(now));
    assert!(limiter.take(now));
    assert!(!limiter.take(now));
    assert!(limiter.take(now + Duration::from_secs(60)));
}

#[test]
fn storage_failure_closes_the_session_with_a_sanitized_error() {
    let (dir, host, owner) = host();
    let mut session = authenticate(&host, &owner);
    std::fs::write(
        dir.path().join("brain-v1.json"),
        b"private corrupt contents",
    )
    .unwrap();
    let response = session.handle(
        Request {
            version: 1,
            id: "request".into(),
            operation: Operation::Invite {},
        },
        NOW,
    );
    assert_eq!(
        serde_json::to_value(&response).unwrap(),
        json!({
            "version":1,"id":"request","result":{"type":"error","data":{"code":"unavailable"}}
        })
    );
    assert!(session.closed());
}

#[test]
fn global_operation_budget_limits_mutations_across_authenticated_sessions() {
    let (_dir, host, owner) = host();
    let mut first = authenticate(&host, &owner);
    let mut second = authenticate(&host, &owner);
    host.operations.lock().unwrap().remaining = 1;
    assert!(matches!(
        call(&mut first, Operation::Pulse {}, NOW),
        Reply::Pulsed { revision: 2 }
    ));
    assert!(matches!(
        call(&mut second, Operation::Pulse {}, NOW),
        Reply::Error(WireError::Busy)
    ));
    assert_eq!(host.store.lock().unwrap().brain().unwrap().revision(), 2);
}

#[test]
fn control_frames_reject_unknown_fields_versions_and_correlation_mismatches() {
    let value = json!({"version":1,"id":"request","operation":{"type":"pulse","data":{}}});
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(Request::decode(&bytes).is_ok());
    let mut nested = value.clone();
    nested["operation"]["data"]["extra"] = json!("private");
    assert!(Request::decode(&serde_json::to_vec(&nested).unwrap()).is_err());
    for (key, value) in [("version", json!(2)), ("extra", json!("private"))] {
        let mut bad: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        bad[key] = value;
        assert!(Request::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    assert!(Request::decode(&vec![0; super::super::reconcile::MAX_CONTROL_BYTES + 1]).is_err());
    let response = Response {
        version: 1,
        id: "request".into(),
        result: Reply::Authenticated {},
    }
    .encode()
    .unwrap();
    assert!(Response::decode(&response, "other-request").is_err());
}
