use super::super::reconcile::SharedData;
use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

fn binding() -> CredentialBinding {
    CredentialBinding {
        brain_id: "brain-1".into(),
        member_id: "member-bob".into(),
        node_id: "node-bob".into(),
        device_id: "device-bob".into(),
    }
}

fn message() -> Message {
    Message::decode(&serde_json::to_vec(&json!({
        "protocolVersion":1,"messageId":"task","idempotencyKey":"task","brainId":"brain-1",
        "senderNodeId":"node-bob","sentAt":1788000000,"revision":2,
        "payload":{"type":"task","data":{"taskId":"task-1","createdByMemberId":"member-bob",
            "goal":"A confirmed goal","constraints":[],"expectedArtifact":"patch","status":"draft"}}
    })).unwrap()).unwrap()
}

fn pages() -> Vec<Page> {
    let records = vec![
        SharedRecord {
            revision: 1,
            data: SharedData::Member {
                member_id: "member-bob".into(),
                identity_id: "bob".into(),
                display_name: "Bob".into(),
                owner: true,
                revoked: false,
            },
        },
        SharedRecord {
            revision: 1,
            data: SharedData::Node {
                node_id: "node-bob".into(),
                member_id: "member-bob".into(),
                device_id: "device-bob".into(),
                online: false,
                last_seen_at: None,
                announcement: None,
            },
        },
        SharedRecord {
            revision: 4,
            data: SharedData::Task {
                task_id: "task-1".into(),
                status: TaskStatus::Running,
                source: message(),
            },
        },
    ];
    records
        .into_iter()
        .enumerate()
        .map(|(offset, record)| Page {
            version: 1,
            brain_id: "brain-1".into(),
            member_id: "member-bob".into(),
            after: 0,
            at: 4,
            offset: offset as u16,
            next: (offset < 2).then_some(offset as u16 + 1),
            records: vec![record],
        })
        .collect()
}

fn ready() -> Node {
    let mut node = Node::new(binding()).unwrap();
    for page in pages() {
        node.accept_page(page).unwrap();
    }
    node
}

#[test]
fn incomplete_or_mixed_pages_never_publish_a_partial_view() {
    let mut node = Node::new(binding()).unwrap();
    let all = pages();
    assert_eq!(
        node.accept_page(all[0].clone()).unwrap(),
        Some(ReconcileRequest {
            after: 0,
            at: Some(4),
            offset: 1
        })
    );
    assert!(!node.is_ready());
    assert!(node.records.is_empty());
    let mut bad = all[1].clone();
    bad.at = 5;
    assert_eq!(node.accept_page(bad), Err(NodeError::Invalid));
    assert_eq!(
        node.reconcile_request(),
        ReconcileRequest {
            after: 0,
            at: None,
            offset: 0
        }
    );
    for page in all {
        node.accept_page(page).unwrap();
    }
    assert_eq!(node.revision(), 4);
    assert_eq!(node.records.len(), 3);
    assert!(node.is_ready());
}

#[test]
fn wrong_identity_duplicate_records_and_inconsistent_membership_are_rejected() {
    for mode in 0..4 {
        let mut node = Node::new(binding()).unwrap();
        let mut all = pages();
        match mode {
            0 => all[0].brain_id = "other-brain".into(),
            1 => all[0].member_id = "other-member".into(),
            2 => all[1].records = all[0].records.clone(),
            3 => {
                if let SharedData::Node { device_id, .. } = &mut all[1].records[0].data {
                    *device_id = "other-device".into();
                }
            }
            _ => unreachable!(),
        }
        let results: Result<Vec<_>, _> =
            all.into_iter().map(|page| node.accept_page(page)).collect();
        assert!(results.is_err());
        assert!(!node.is_ready());
        assert!(node.records.is_empty());
    }
}

#[test]
fn disconnect_keeps_revision_and_marks_running_view_unknown_without_execution() {
    let mut node = ready();
    let before = node.records.clone();
    assert_eq!(node.task_status("task-1"), Some(TaskStatus::Running));
    node.disconnect();
    assert_eq!(node.task_status("task-1"), Some(TaskStatus::OutcomeUnknown));
    assert_eq!(node.records, before);
    assert_eq!(
        node.reconcile_request(),
        ReconcileRequest {
            after: 4,
            at: None,
            offset: 0
        }
    );
    node.accept_page(Page {
        version: 1,
        brain_id: "brain-1".into(),
        member_id: "member-bob".into(),
        after: 4,
        at: 4,
        offset: 0,
        next: None,
        records: vec![],
    })
    .unwrap();
    assert_eq!(node.task_status("task-1"), Some(TaskStatus::OutcomeUnknown));
}

#[test]
fn stale_revision_cannot_roll_back_the_completed_view() {
    let mut node = ready();
    let before = node.records.clone();
    assert_eq!(node.accept_page(pages().remove(0)), Err(NodeError::Invalid));
    assert_eq!(node.records, before);
    assert_eq!(node.revision(), 4);
    assert!(!node.is_ready());
}

#[test]
fn retry_retains_the_key_and_payload_and_ack_requires_matching_pending_operation() {
    let mut node = ready();
    let original = message();
    node.queue_confirmed(original.clone()).unwrap();
    let first = node.outgoing().unwrap();
    assert_eq!(first.admission().revision, 4);
    assert_eq!(first.payload_hash(), original.payload_hash());
    assert_eq!(
        node.acknowledge("other", /*revision*/ 5),
        Err(NodeError::Invalid)
    );
    node.disconnect();
    assert_eq!(node.outgoing(), Err(NodeError::Unavailable));
    node.accept_page(Page {
        version: 1,
        brain_id: "brain-1".into(),
        member_id: "member-bob".into(),
        after: 4,
        at: 5,
        offset: 0,
        next: None,
        records: vec![],
    })
    .unwrap();
    let retry = node.outgoing().unwrap();
    assert_eq!(retry.admission().key, first.admission().key);
    assert_eq!(retry.payload_hash(), first.payload_hash());
    node.acknowledge("task", /*revision*/ 5).unwrap();
    assert!(!node.is_ready());
    assert!(node.pending_message.is_none());
}

#[test]
fn a_second_operation_and_foreign_sender_cannot_replace_the_pending_message() {
    let mut node = ready();
    let original = message();
    node.queue_confirmed(original.clone()).unwrap();
    node.queue_confirmed(original.clone()).unwrap();
    let mut value = serde_json::to_value(&original).unwrap();
    value["idempotencyKey"] = json!("other");
    assert_eq!(
        node.queue_confirmed(Message::decode(&serde_json::to_vec(&value).unwrap()).unwrap()),
        Err(NodeError::Busy)
    );
    value["senderNodeId"] = json!("other-node");
    assert_eq!(
        node.queue_confirmed(Message::decode(&serde_json::to_vec(&value).unwrap()).unwrap()),
        Err(NodeError::Invalid)
    );
    assert_eq!(node.pending_message, Some(original));
}

#[test]
fn completed_receipt_retry_preserves_the_confirmed_content_hash() {
    let receipt = Message::decode(&serde_json::to_vec(&json!({
        "protocolVersion":1,"messageId":"receipt","idempotencyKey":"receipt","brainId":"brain-1",
        "senderNodeId":"node-bob","sentAt":1788000100,"revision":7,
        "payload":{"type":"runReceipt","data":{"content":{
            "taskId":"task-1","runId":"run-1","nodeId":"node-bob","runtimeId":"codex","runtimeVersion":"1.2.3",
            "startedAt":1788000000,"finishedAt":1788000090,"outcome":"success",
            "summary":"Requested change is ready.","failure":null,"tests":{"state":"notReported"},
            "artifact":{"artifactId":"artifact-1","taskId":"task-1","runId":"run-1","baselineCommit":"a".repeat(40),
                "state":"empty","limitBytes":524288,"byteCount":0,"sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}
        },"contentSha256":"b22ed69ff79dccb1da4985a875c30e7aae06fa5aa1c0d46b2538e66ffd56fe40"}}
    })).unwrap()).unwrap();
    let mut node = ready();
    node.queue_confirmed(receipt.clone()).unwrap();
    let before = node.outgoing().unwrap().payload_hash().unwrap();
    node.disconnect();
    assert_eq!(
        node.pending_message
            .as_ref()
            .unwrap()
            .payload_hash()
            .unwrap(),
        before
    );
    assert_eq!(node.pending_message.unwrap(), receipt);
}
