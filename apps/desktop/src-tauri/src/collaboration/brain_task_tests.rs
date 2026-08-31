use super::super::brain::{OwnerProfile, PresenceReset};
use super::super::credential::{AccessError, IssuedCredential};
use super::super::invitation::JoinRequest;
use super::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

const NOW: i64 = 1_788_000_000;

fn setup() -> (Brain, IssuedCredential, ConnectionIdentity) {
    let (mut brain, credential) = Brain::bootstrap(
        "brain-1".into(),
        OwnerProfile {
            identity_id: "owner",
            device_id: "device-1",
            display_name: "Owner",
        },
        NOW,
    )
    .unwrap();
    let session = brain
        .connect(&credential.binding, &credential.secret, NOW)
        .unwrap();
    (brain, credential, session)
}

fn message(credential: &IssuedCredential, revision: u64, key: &str, payload: Value) -> Message {
    Message::decode(
        &serde_json::to_vec(&json!({
            "protocolVersion": 1, "messageId": format!("msg-{key}"), "idempotencyKey": key,
            "brainId": "brain-1", "senderNodeId": credential.binding.node_id, "sentAt": NOW,
            "revision": revision, "payload": payload,
        }))
        .unwrap(),
    )
    .unwrap()
}

fn node(credential: &IssuedCredential) -> Value {
    json!({"type":"node","data":{
        "nodeId":credential.binding.node_id, "memberId":credential.binding.member_id,
        "deviceId":credential.binding.device_id, "runtimeId":"codex","runtimeVersion":"test","capabilities":["taskRun"],
    }})
}

fn task(credential: &IssuedCredential, task_id: &str) -> Value {
    json!({"type":"task","data":{
        "taskId":task_id, "createdByMemberId":credential.binding.member_id, "goal":"bounded task",
        "constraints":[], "expectedArtifact":"patch", "status":"draft",
    }})
}

#[test]
fn duplicate_heartbeat_returns_original_revision_without_reviving_presence() {
    let (mut brain, credential, session) = setup();
    let first = message(
        &credential,
        brain.revision(),
        "heartbeat-1",
        node(&credential),
    );
    assert_eq!(brain.apply(&session, first.clone(), NOW), Ok(2));
    brain
        .reset_presence(NOW + 30, PresenceReset::Expired)
        .unwrap();
    brain
        .connect(&credential.binding, &credential.secret, NOW + 30)
        .unwrap();
    let before = brain.encode().unwrap();
    assert_eq!(brain.apply(&session, first, NOW + 30), Ok(2));
    let retry = message(
        &credential,
        brain.revision(),
        "heartbeat-1",
        node(&credential),
    );
    assert_eq!(brain.apply(&session, retry, NOW + 30), Ok(2));
    assert_eq!(brain.encode().unwrap(), before);
    let mut changed = node(&credential);
    changed["data"]["runtimeVersion"] = json!("changed");
    let conflicting = message(&credential, brain.revision(), "heartbeat-1", changed);
    assert_eq!(
        brain.apply(&session, conflicting, NOW + 30),
        Err(BrainError::Conflict)
    );
    let stale = message(
        &credential,
        /*revision*/ 1,
        "heartbeat-2",
        node(&credential),
    );
    assert_eq!(
        brain.apply(&session, stale, NOW + 30),
        Err(BrainError::Conflict)
    );
    assert!(!brain.state.nodes[&credential.binding.node_id].online);
}

#[test]
fn task_status_never_rolls_back_or_changes_due_to_presence_loss() {
    let (mut brain, credential, session) = setup();
    let create = message(
        &credential,
        brain.revision(),
        "create-1",
        task(&credential, "task-1"),
    );
    brain.apply(&session, create.clone(), NOW).unwrap();
    for next in [
        TaskStatus::Offered,
        TaskStatus::Accepted,
        TaskStatus::Running,
    ] {
        brain
            .set_task_status("task-1", brain.revision(), next, NOW)
            .unwrap();
    }
    let heartbeat = message(
        &credential,
        brain.revision(),
        "heartbeat",
        node(&credential),
    );
    brain.apply(&session, heartbeat, NOW).unwrap();
    brain
        .reset_presence(NOW + 30, PresenceReset::Expired)
        .unwrap();
    assert_eq!(brain.state.tasks["task-1"].status, TaskStatus::Running);
    assert_eq!(brain.apply(&session, create, NOW + 30), Ok(2));
    assert_eq!(
        brain.set_task_status("task-1", brain.revision(), TaskStatus::Draft, NOW + 30),
        Err(BrainError::Invalid)
    );
    let mut restored = Brain::decode(&brain.encode().unwrap(), "brain-1").unwrap();
    restored
        .reset_presence(NOW + 30, PresenceReset::Restart)
        .unwrap();
    assert_eq!(restored.state.tasks["task-1"].status, TaskStatus::Running);
}

#[test]
fn peers_cannot_claim_other_members_or_write_run_and_owner_state() {
    let (mut brain, credential, session) = setup();
    let mut forged_node = node(&credential);
    forged_node["data"]["memberId"] = json!("someone-else");
    let mut forged_task = task(&credential, "task-1");
    forged_task["data"]["createdByMemberId"] = json!("someone-else");
    let mut success = task(&credential, "task-2");
    success["data"]["status"] = json!("approved");
    let owner_claim = json!({"type":"identity","data":{
        "identityId":"attacker","memberId":credential.binding.member_id,
        "deviceId":credential.binding.device_id,"displayName":"Owner","role":"owner",
    }});
    let before = brain.encode().unwrap();
    for payload in [forged_node, forged_task, success, owner_claim] {
        let forged = message(&credential, brain.revision(), "forged", payload);
        assert_eq!(brain.apply(&session, forged, NOW), Err(BrainError::Invalid));
    }
    assert_eq!(brain.encode().unwrap(), before);
}

#[test]
fn replay_domain_is_per_sender_and_revocation_still_denies_retries() {
    let (mut brain, owner, owner_session) = setup();
    let invitation = brain.create_invitation(NOW).unwrap();
    let joined = brain
        .join(
            JoinRequest {
                brain_id: "brain-1",
                invitation_id: &invitation.invitation_id,
                secret: &invitation.secret,
                identity_id: "member",
                device_id: "device-2",
                display_name: "Member",
            },
            NOW,
        )
        .unwrap();
    let peer = brain
        .connect(&joined.credential.binding, &joined.credential.secret, NOW)
        .unwrap();
    let first = message(&owner, brain.revision(), "same-key", node(&owner));
    brain.apply(&owner_session, first, NOW).unwrap();
    let second = message(
        &joined.credential,
        brain.revision(),
        "same-key",
        node(&joined.credential),
    );
    brain.apply(&peer, second.clone(), NOW).unwrap();
    brain
        .revoke_member(&joined.member.member_id, brain.revision(), NOW)
        .unwrap();
    assert_eq!(
        brain.apply(&peer, second, NOW),
        Err(BrainError::Access(AccessError::Rejected))
    );
}

#[test]
fn replay_capacity_is_not_silently_evicted_and_snapshot_replays_survive() {
    let (mut brain, credential, session) = setup();
    let first = message(
        &credential,
        brain.revision(),
        "heartbeat-0",
        node(&credential),
    );
    brain.apply(&session, first.clone(), NOW).unwrap();
    for index in 1..MAX_REPLAYS {
        let heartbeat = message(
            &credential,
            brain.revision(),
            &format!("heartbeat-{index}"),
            node(&credential),
        );
        brain.apply(&session, heartbeat, NOW).unwrap();
    }
    let extra = message(&credential, brain.revision(), "extra", node(&credential));
    assert_eq!(brain.apply(&session, extra, NOW), Err(BrainError::Capacity));
    let mut restored = Brain::decode(&brain.encode().unwrap(), "brain-1").unwrap();
    assert_eq!(restored.apply(&session, first, NOW), Ok(2));
    let mut invalid = serde_json::to_value(&restored).unwrap();
    let duplicate = invalid["replays"][0].clone();
    invalid["replays"][1] = duplicate;
    assert!(Brain::decode(&serde_json::to_vec(&invalid).unwrap(), "brain-1").is_err());
}

#[test]
fn task_capacity_rejects_new_work_without_partial_state() {
    let (mut brain, credential, session) = setup();
    for index in 0..MAX_TASKS {
        let key = format!("task-{index}");
        let create = message(&credential, brain.revision(), &key, task(&credential, &key));
        brain.apply(&session, create, NOW).unwrap();
    }
    let before = brain.encode().unwrap();
    let extra = message(
        &credential,
        brain.revision(),
        "extra",
        task(&credential, "extra"),
    );
    assert_eq!(brain.apply(&session, extra, NOW), Err(BrainError::Capacity));
    assert_eq!(brain.encode().unwrap(), before);
}

#[test]
fn existing_task_and_authenticated_envelope_cannot_be_reassigned() {
    let (mut brain, credential, session) = setup();
    let original = message(
        &credential,
        brain.revision(),
        "create",
        task(&credential, "task-1"),
    );
    brain.apply(&session, original, NOW).unwrap();
    let replacement = message(
        &credential,
        brain.revision(),
        "replace",
        task(&credential, "task-1"),
    );
    assert_eq!(
        brain.apply(&session, replacement, NOW),
        Err(BrainError::Conflict)
    );
    let before = brain.encode().unwrap();
    let valid = message(
        &credential,
        brain.revision(),
        "other",
        task(&credential, "task-2"),
    );
    for field in ["senderNodeId", "brainId"] {
        let mut value = serde_json::to_value(&valid).unwrap();
        value[field] = json!("other");
        let forged = Message::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(brain.apply(&session, forged, NOW), Err(BrainError::Invalid));
    }
    assert_eq!(brain.encode().unwrap(), before);
}
