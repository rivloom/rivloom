use super::super::protocol::TaskStatus;
use super::super::reconcile::SharedData;
use super::super::test_support::{Fixture, Memory, fixture};
use super::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use zeroize::Zeroizing;

fn owner(f: &Fixture) -> (Client, NodeSecrets<Memory>) {
    let vault = NodeSecrets::new(Memory::default());
    vault.save_new(&f.owner, f.now).unwrap();
    let client = Client::connect(&f.peer, &vault, f.owner.binding.clone()).unwrap();
    (client, vault)
}

fn join<B: SecretBackend>(
    f: &Fixture,
    vault: &NodeSecrets<B>,
    invitation: &IssuedInvitation,
) -> Result<Client, ClientError> {
    Client::join(
        &f.peer,
        vault,
        JoinRequest {
            brain_id: &invitation.brain_id,
            invitation_id: &invitation.invitation_id,
            secret: &invitation.secret,
            identity_id: "bob",
            device_id: "bob-device",
            display_name: "Bob",
        },
    )
}

fn message(client: &Client, key: &str, payload: Value) -> Message {
    Message::decode(
        &serde_json::to_vec(&json!({
            "protocolVersion":1,"messageId":key,"idempotencyKey":key,
            "brainId":client.binding().brain_id,"senderNodeId":client.binding().node_id,
            "sentAt":now().unwrap(),"revision":client.view().revision(),"payload":payload
        }))
        .unwrap(),
    )
    .unwrap()
}

fn announcement(client: &Client) -> Message {
    message(
        client,
        "announce",
        json!({"type":"node","data":{
            "nodeId":client.binding().node_id,"memberId":client.binding().member_id,
            "deviceId":client.binding().device_id,"runtimeId":"codex","runtimeVersion":"fixture",
            "capabilities":["taskRun","interrupt","patch"]
        }}),
    )
}

fn task(client: &Client, key: &str) -> Message {
    message(
        client,
        key,
        json!({"type":"task","data":{
            "taskId":key,"createdByMemberId":client.binding().member_id,"goal":"Explicitly shared draft",
            "constraints":[],"expectedArtifact":"patch","status":"draft"
        }}),
    )
}

#[test]
fn two_native_tls_nodes_enroll_announce_and_only_receive_their_own_tasks() {
    let f = fixture();
    let (mut alice, _alice_vault) = owner(&f);
    let invitation = alice.invite().unwrap();
    let bob_vault = NodeSecrets::new(Memory::default());
    let mut bob = join(&f, &bob_vault, &invitation).unwrap();
    assert_eq!(
        bob_vault
            .load(bob.binding(), now().unwrap())
            .unwrap()
            .binding,
        *bob.binding()
    );
    alice.reconcile().unwrap();
    let alice_node = announcement(&alice);
    alice.submit_confirmed(alice_node.clone()).unwrap();
    bob.reconcile().unwrap();
    let bob_node = announcement(&bob);
    bob.submit_confirmed(bob_node.clone()).unwrap();
    alice.reconcile().unwrap();
    for client in [&alice, &bob] {
        let mut actual: Vec<_> = client
            .view()
            .shared_records()
            .filter_map(|record| match &record.data {
                SharedData::Node {
                    announcement: Some(message),
                    ..
                } => Some(message.clone()),
                _ => None,
            })
            .collect();
        let mut expected = vec![alice_node.clone(), bob_node.clone()];
        actual.sort_by_key(|message| message.admission().sender_node_id.to_owned());
        expected.sort_by_key(|message| message.admission().sender_node_id.to_owned());
        assert_eq!(actual, expected);
    }
    alice.submit_confirmed(task(&alice, "alice-task")).unwrap();
    bob.reconcile().unwrap();
    bob.submit_confirmed(task(&bob, "bob-task")).unwrap();
    alice.reconcile().unwrap();
    assert_eq!(
        (
            alice.view().task_status("alice-task"),
            alice.view().task_status("bob-task"),
            bob.view().task_status("alice-task"),
            bob.view().task_status("bob-task")
        ),
        (Some(TaskStatus::Draft), None, None, Some(TaskStatus::Draft))
    );
    assert_eq!(alice.view().revision(), bob.view().revision());
    bob.pulse().unwrap();
}

#[test]
fn discarded_acknowledgement_reconnects_and_replays_without_a_second_mutation() {
    let f = fixture();
    let (mut alice, vault) = owner(&f);
    let confirmed = task(&alice, "confirmed-task");
    let before = alice.view().revision();
    alice.node.queue_confirmed(confirmed.clone()).unwrap();
    // Discard the application acknowledgement after a real durable TLS Submit.
    let Reply::Applied { key, revision } = alice
        .request(Operation::Submit(Box::new(alice.node.outgoing().unwrap())))
        .unwrap()
    else {
        panic!("submit");
    };
    assert_eq!((key.as_str(), revision), ("confirmed-task", before + 1));
    alice.disconnect();
    assert!(!alice.view().is_ready());
    assert_eq!(alice.view().revision(), before);
    alice.reconnect(&f.peer, &vault).unwrap();
    assert_eq!(alice.view().revision(), revision);
    let retry = alice.node.outgoing().unwrap();
    assert_eq!(
        (retry.admission().key, retry.payload_hash()),
        (confirmed.admission().key, confirmed.payload_hash())
    );
    assert_eq!(alice.publish_pending(), Ok(revision));
    assert_eq!(alice.view().revision(), revision);
    assert_eq!(
        alice.view().task_status("confirmed-task"),
        Some(TaskStatus::Draft)
    );
    assert_eq!(
        alice.publish_pending(),
        Err(ClientError::Node(NodeError::Unavailable))
    );
}

#[test]
fn revoked_node_loses_both_its_live_session_and_reconnect_access() {
    let f = fixture();
    let (mut alice, _vault) = owner(&f);
    let bob_vault = NodeSecrets::new(Memory::default());
    let mut bob = join(&f, &bob_vault, &alice.invite().unwrap()).unwrap();
    alice.reconcile().unwrap();
    alice.revoke(bob.binding().member_id.clone()).unwrap();
    assert_eq!(bob.pulse(), Err(ClientError::Peer(WireError::Rejected)));
    assert!(!bob.view().is_ready());
    assert_eq!(
        bob.reconnect(&f.peer, &bob_vault),
        Err(ClientError::Peer(WireError::Rejected))
    );
    assert!(!bob.view().is_ready());
    alice.pulse().unwrap();
}

#[test]
fn listener_loss_preserves_the_completed_revision_but_clears_readiness() {
    let mut f = fixture();
    let (mut alice, vault) = owner(&f);
    let revision = alice.view().revision();
    f.server.stop();
    assert_eq!(alice.pulse(), Err(ClientError::Transport));
    assert_eq!(
        (alice.view().revision(), alice.view().is_ready()),
        (revision, false)
    );
    assert_eq!(
        alice.reconnect(&f.peer, &vault),
        Err(ClientError::Transport)
    );
}

struct Unavailable;
impl SecretBackend for Unavailable {
    fn read(&self, _target: &str) -> Result<Option<Zeroizing<Vec<u8>>>, VaultError> {
        Err(VaultError::Unavailable)
    }
    fn write_new(&self, _target: &str, _bytes: &[u8]) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
    fn remove(&self, _target: &str) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
}

#[test]
fn failed_vault_save_does_not_report_join_success_or_redeem_the_invitation_again() {
    let f = fixture();
    let (mut alice, _vault) = owner(&f);
    let invitation = alice.invite().unwrap();
    let unavailable = NodeSecrets::new(Unavailable);
    assert!(matches!(
        join(&f, &unavailable, &invitation),
        Err(ClientError::Vault(VaultError::Unavailable))
    ));
    alice.reconcile().unwrap();
    let orphan: Vec<_> = alice
        .view()
        .shared_records()
        .filter_map(|record| match &record.data {
            SharedData::Member {
                member_id,
                identity_id,
                revoked: false,
                ..
            } if identity_id == "bob" => Some(member_id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(orphan.len(), 1);
    assert!(matches!(
        join(&f, &NodeSecrets::new(Memory::default()), &invitation),
        Err(ClientError::Peer(WireError::Rejected))
    ));
    alice.revoke(orphan[0].clone()).unwrap();
}

#[test]
fn cancelled_invitation_and_member_admin_requests_are_rejected() {
    let f = fixture();
    let (mut alice, _vault) = owner(&f);
    let cancelled = alice.invite().unwrap();
    alice
        .cancel_invite(cancelled.invitation_id.clone())
        .unwrap();
    let bob_vault = NodeSecrets::new(Memory::default());
    assert!(matches!(
        join(&f, &bob_vault, &cancelled),
        Err(ClientError::Peer(WireError::Rejected))
    ));
    let mut bob = join(&f, &bob_vault, &alice.invite().unwrap()).unwrap();
    assert!(matches!(
        bob.invite(),
        Err(ClientError::Peer(WireError::Rejected))
    ));
    assert!(!bob.view().is_ready());
}

#[test]
fn revision_conflict_preserves_the_confirmed_message_for_explicit_reconnect() {
    let f = fixture();
    let (mut alice, alice_vault) = owner(&f);
    let bob_vault = NodeSecrets::new(Memory::default());
    let mut bob = join(&f, &bob_vault, &alice.invite().unwrap()).unwrap();
    alice.reconcile().unwrap();
    let confirmed = task(&alice, "alice-conflict");
    bob.submit_confirmed(task(&bob, "bob-first")).unwrap();
    assert_eq!(
        alice.submit_confirmed(confirmed.clone()),
        Err(ClientError::Peer(WireError::Conflict))
    );
    assert!(!alice.view().is_ready());
    alice.reconnect(&f.peer, &alice_vault).unwrap();
    assert_eq!(
        alice.node.outgoing().unwrap().payload_hash(),
        confirmed.payload_hash()
    );
    alice.publish_pending().unwrap();
    assert_eq!(
        alice.view().task_status("alice-conflict"),
        Some(TaskStatus::Draft)
    );
}
