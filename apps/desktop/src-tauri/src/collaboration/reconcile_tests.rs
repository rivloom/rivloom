use super::super::brain::{Brain, OwnerProfile};
use super::super::credential::{IssuedCredential, SecretToken};
use super::super::invitation::JoinRequest;
use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

const NOW: i64 = 1_788_000_000;

fn setup() -> (Brain, IssuedCredential, IssuedCredential) {
    let (mut brain, owner) = Brain::bootstrap(
        "brain-1".into(),
        OwnerProfile {
            identity_id: "alice",
            device_id: "alice-device",
            display_name: "Alice",
        },
        NOW,
    )
    .unwrap();
    let invite = brain.create_invitation(NOW).unwrap();
    let joined = brain
        .join(
            JoinRequest {
                brain_id: "brain-1",
                invitation_id: &invite.invitation_id,
                secret: &invite.secret,
                identity_id: "bob",
                device_id: "bob-device",
                display_name: "Bob",
            },
            NOW,
        )
        .unwrap();
    (brain, owner, joined.credential)
}

fn task(brain: &Brain, credential: &IssuedCredential, name: &str) -> Message {
    Message::decode(&serde_json::to_vec(&json!({
        "protocolVersion":1,"messageId":name,"idempotencyKey":name,"brainId":"brain-1",
        "senderNodeId":credential.binding.node_id,"sentAt":NOW,"revision":brain.revision(),
        "payload":{"type":"task","data":{"taskId":name,"createdByMemberId":credential.binding.member_id,
            "goal":format!("private-{name}"),"constraints":[],"expectedArtifact":"patch","status":"draft"}}
    })).unwrap()).unwrap()
}

fn collect(brain: &mut Brain, credential: &IssuedCredential, after: u64) -> Vec<SharedRecord> {
    let session = brain
        .connect(&credential.binding, &credential.secret, NOW)
        .unwrap();
    let mut request = ReconcileRequest {
        after,
        at: None,
        offset: 0,
    };
    let mut records = Vec::new();
    loop {
        let page = brain.reconcile(&session, request.clone(), NOW).unwrap();
        assert_eq!(Page::decode(&page.encode().unwrap()).unwrap(), page);
        records.extend(page.records);
        match page.next {
            Some(offset) => {
                request = ReconcileRequest {
                    after,
                    at: Some(page.at),
                    offset,
                }
            }
            None => break,
        }
    }
    records
}

#[test]
fn projection_shares_members_but_not_other_members_tasks_or_authority_secrets() {
    let (mut brain, alice, bob) = setup();
    for (credential, name) in [(&alice, "alice-task"), (&bob, "bob-task")] {
        let session = brain
            .connect(&credential.binding, &credential.secret, NOW)
            .unwrap();
        let message = task(&brain, credential, name);
        brain.apply(&session, message, NOW).unwrap();
    }
    let records = collect(&mut brain, &alice, /*after*/ 0);
    assert_eq!(records.len(), 5);
    let bytes = serde_json::to_string(&records).unwrap();
    assert!(bytes.contains("private-alice-task"));
    for forbidden in [
        "private-bob-task",
        "verifier",
        "invitations",
        "credentials",
        alice.secret.expose_secret(),
        bob.secret.expose_secret(),
    ] {
        assert!(!bytes.contains(forbidden));
    }
    assert_eq!(collect(&mut brain, &bob, /*after*/ 0).len(), 5);
}

#[test]
fn incremental_pages_return_latest_changes_and_never_mix_revisions() {
    let (mut brain, alice, bob) = setup();
    let session = brain.connect(&alice.binding, &alice.secret, NOW).unwrap();
    let before = brain.revision();
    assert!(collect(&mut brain, &alice, before).is_empty());
    let page = brain
        .reconcile(
            &session,
            ReconcileRequest {
                after: 0,
                at: None,
                offset: 0,
            },
            NOW,
        )
        .unwrap();
    brain
        .revoke_member(&bob.binding.member_id, before, NOW)
        .unwrap();
    assert_eq!(
        brain.reconcile(
            &session,
            ReconcileRequest {
                after: 0,
                at: Some(page.at),
                offset: page.next.unwrap(),
            },
            NOW
        ),
        Err(BrainError::Conflict)
    );
    let changed = collect(&mut brain, &alice, before);
    assert_eq!(changed.len(), 2);
    assert!(changed.iter().all(|record| record.revision == before + 1));
}

#[test]
fn revocation_and_expiry_are_rechecked_on_every_page() {
    let (mut brain, alice, bob) = setup();
    let session = brain.connect(&bob.binding, &bob.secret, NOW).unwrap();
    let first = brain
        .reconcile(
            &session,
            ReconcileRequest {
                after: 0,
                at: None,
                offset: 0,
            },
            NOW,
        )
        .unwrap();
    brain
        .revoke_member(&bob.binding.member_id, brain.revision(), NOW)
        .unwrap();
    assert!(
        brain
            .reconcile(
                &session,
                ReconcileRequest {
                    after: 0,
                    at: Some(first.at),
                    offset: first.next.unwrap(),
                },
                NOW
            )
            .is_err()
    );
    let owner = brain.connect(&alice.binding, &alice.secret, NOW).unwrap();
    assert!(
        brain
            .reconcile(
                &owner,
                ReconcileRequest {
                    after: 0,
                    at: None,
                    offset: 0
                },
                alice.expires_at
            )
            .is_err()
    );
}

#[test]
fn invalid_cursors_and_wire_pages_fail_closed() {
    let (mut brain, alice, _) = setup();
    let session = brain.connect(&alice.binding, &alice.secret, NOW).unwrap();
    for request in [
        ReconcileRequest {
            after: brain.revision() + 1,
            at: None,
            offset: 0,
        },
        ReconcileRequest {
            after: 0,
            at: None,
            offset: 1,
        },
        ReconcileRequest {
            after: 0,
            at: Some(brain.revision()),
            offset: 193,
        },
    ] {
        assert!(brain.reconcile(&session, request, NOW).is_err());
    }
    let page = brain
        .reconcile(
            &session,
            ReconcileRequest {
                after: 0,
                at: None,
                offset: 0,
            },
            NOW,
        )
        .unwrap();
    let original = serde_json::to_value(&page).unwrap();
    for (key, value) in [
        ("version", json!(2)),
        ("at", json!(0)),
        ("memberId", json!("../owner")),
        (
            "secret",
            json!(SecretToken::generate().unwrap().expose_secret()),
        ),
    ] {
        let mut bad = original.clone();
        bad[key] = value;
        assert!(Page::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    assert!(Page::decode(&vec![b' '; MAX_CONTROL_BYTES + 1]).is_err());
}

#[test]
fn stale_presence_is_reconciled_without_changing_task_outcome() {
    let (mut brain, alice, _) = setup();
    let session = brain.connect(&alice.binding, &alice.secret, NOW).unwrap();
    let message = task(&brain, &alice, "task");
    brain.apply(&session, message, NOW).unwrap();
    brain.heartbeat(&session, brain.revision(), NOW).unwrap();
    let before = brain.revision();
    let page = brain
        .reconcile(
            &session,
            ReconcileRequest {
                after: before,
                at: None,
                offset: 0,
            },
            NOW + 30,
        )
        .unwrap();
    assert_eq!(page.at, before + 1);
    assert!(matches!(
        &page.records[0].data,
        SharedData::Node { online: false, .. }
    ));
    assert_eq!(brain.state.tasks["task"].status, TaskStatus::Draft);
}
