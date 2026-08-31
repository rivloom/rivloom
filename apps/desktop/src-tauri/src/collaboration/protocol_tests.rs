use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::*;

fn payloads() -> Vec<Value> {
    vec![
        json!({"type": "identity", "data": {
            "identityId": "identity-bob", "memberId": "member-bob", "deviceId": "device-bob",
            "displayName": "Bob", "role": "member"
        }}),
        json!({"type": "node", "data": {
            "nodeId": "node-bob", "memberId": "member-bob", "deviceId": "device-bob",
            "runtimeId": "codex", "runtimeVersion": "1.2.3",
            "capabilities": ["taskRun", "interrupt", "patch"]
        }}),
        json!({"type": "task", "data": {
            "taskId": "task-1", "createdByMemberId": "member-alice",
            "goal": "Fix the parser", "constraints": ["Keep the public API"],
            "expectedArtifact": "patch", "status": "offered"
        }}),
        json!({"type": "assignment", "data": {
            "assignmentId": "assignment-1", "taskId": "task-1",
            "offeredByMemberId": "member-alice", "targetNodeId": "node-bob",
            "executionPolicy": "managedWorktreeOffline",
            "decision": {"state": "accepted", "acceptedByMemberId": "member-bob",
                "projectRef": "project-1", "runId": "run-1", "runKey": "run-key-1",
                "acceptedAt": 1788000000}
        }}),
    ]
}

fn envelope(payload: Value) -> Value {
    json!({
        "protocolVersion": 1, "messageId": "message-1", "idempotencyKey": "request-1",
        "brainId": "brain-1", "senderNodeId": "node-bob",
        "sentAt": 1788000100, "revision": 7, "payload": payload
    })
}

fn decode(value: &Value) -> Result<Message, ProtocolError> {
    Message::decode(&serde_json::to_vec(value).unwrap())
}

#[test]
fn golden_messages_round_trip_without_local_runtime_data() {
    for payload in payloads() {
        let golden = envelope(payload);
        let message = decode(&golden).unwrap();
        let encoded = message.encode().unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), golden);
        assert_eq!(Message::decode(&encoded).unwrap(), message);
        let wire = String::from_utf8(encoded).unwrap();
        for forbidden in [
            "Token",
            "cwd",
            "CODEX_HOME",
            "environment",
            "logs",
            "appServer",
            "C:\\",
            "/home/",
        ] {
            assert!(!wire.contains(forbidden), "{forbidden}");
        }
    }
}

#[test]
fn versions_headers_and_unknown_fields_fail_closed() {
    for payload in payloads() {
        for (field, value) in [
            ("protocolVersion", json!(0)),
            ("protocolVersion", json!(2)),
            ("protocolVersion", json!("1")),
            ("messageId", json!("")),
            ("idempotencyKey", json!("a".repeat(129))),
            ("brainId", json!("/home/bob")),
            ("senderNodeId", json!("C:\\private")),
            ("sentAt", json!(-1)),
            ("sentAt", json!(1.5)),
            ("revision", json!(9007199254740992u64)),
            ("runtimeToken", json!("synthetic-secret")),
        ] {
            let mut invalid = envelope(payload.clone());
            invalid[field] = value;
            assert_eq!(
                decode(&invalid),
                Err(ProtocolError::InvalidMessage),
                "{field}"
            );
            assert!(serde_json::from_value::<Message>(invalid).is_err());
        }
        let mut invalid = envelope(payload);
        invalid["payload"]["data"]["cwd"] = json!("C:\\private");
        assert_eq!(decode(&invalid), Err(ProtocolError::InvalidMessage));
    }
}

#[test]
fn malformed_duplicate_and_oversized_frames_are_rejected() {
    for bytes in [b"{".as_slice(), b"[]", b"null", b"\xff"] {
        assert_eq!(Message::decode(bytes), Err(ProtocolError::InvalidMessage));
    }
    let mut bytes = serde_json::to_vec(&envelope(payloads().remove(0))).unwrap();
    bytes.resize(MAX_MESSAGE_BYTES, b' ');
    assert!(Message::decode(&bytes).is_ok());
    bytes.push(b' ');
    assert_eq!(Message::decode(&bytes), Err(ProtocolError::MessageTooLarge));
    let wire = serde_json::to_string(&envelope(payloads().remove(0))).unwrap();
    let duplicate = wire.replacen(
        "\"protocolVersion\":1",
        "\"protocolVersion\":1,\"protocolVersion\":1",
        1,
    );
    assert_eq!(
        Message::decode(duplicate.as_bytes()),
        Err(ProtocolError::InvalidMessage)
    );
}

#[test]
fn text_and_collection_limits_apply_on_receive_and_send() {
    let mut task = envelope(payloads().remove(2));
    task["payload"]["data"]["goal"] = json!("界".repeat(1365) + "a");
    task["payload"]["data"]["constraints"] = json!(vec!["a".repeat(256); 32]);
    assert!(decode(&task).is_ok());
    for (field, value) in [
        ("goal", json!("界".repeat(1366))),
        ("goal", json!("  ")),
        ("constraints", json!(vec!["x"; 33])),
        ("constraints", json!(vec!["x".repeat(1025)])),
        ("constraints", json!(vec!["x".repeat(1024); 9])),
    ] {
        let mut invalid = task.clone();
        invalid["payload"]["data"][field] = value;
        assert_eq!(decode(&invalid), Err(ProtocolError::InvalidMessage));
        let raw: Envelope = serde_json::from_value(invalid).unwrap();
        assert_eq!(Message::try_from(raw), Err(ProtocolError::InvalidMessage));
    }
    let mut identity = envelope(payloads().remove(0));
    identity["payload"]["data"]["displayName"] = json!("界".repeat(27));
    assert_eq!(decode(&identity), Err(ProtocolError::InvalidMessage));
    let mut node = envelope(payloads().remove(1));
    node["payload"]["data"]["capabilities"] = json!(["taskRun", "taskRun"]);
    assert_eq!(decode(&node), Err(ProtocolError::InvalidMessage));
}

#[test]
fn assignment_acceptance_cannot_supply_paths_or_expand_permissions() {
    for decision in [
        json!({"state": "offered"}),
        json!({"state": "rejected", "decidedByMemberId": "member-bob", "decidedAt": 1788000000}),
        json!({"state": "cancelled", "decidedByMemberId": "member-alice", "decidedAt": 1788000000}),
    ] {
        let mut value = envelope(payloads().remove(3));
        value["payload"]["data"]["decision"] = decision;
        assert!(decode(&value).is_ok());
    }
    for (field, value) in [
        ("projectRef", json!("../checkout")),
        ("projectRef", json!("\\\\server\\share")),
        ("runKey", json!("")),
        ("acceptedAt", json!(-1)),
        ("networkAccess", json!(true)),
        ("runtimeToken", json!("synthetic-secret")),
    ] {
        let mut invalid = envelope(payloads().remove(3));
        invalid["payload"]["data"]["decision"][field] = value;
        assert_eq!(decode(&invalid), Err(ProtocolError::InvalidMessage));
    }
    let mut invalid = envelope(payloads().remove(3));
    invalid["payload"]["data"]["executionPolicy"] = json!("unrestricted");
    assert_eq!(decode(&invalid), Err(ProtocolError::InvalidMessage));
}

#[test]
fn unit_variants_and_tag_wrappers_reject_unrecognized_fields() {
    let mut assignment = envelope(payloads().remove(3));
    assignment["payload"]["data"]["decision"] = json!({"state": "offered", "networkAccess": true});
    assert_eq!(decode(&assignment), Err(ProtocolError::InvalidMessage));
    let mut identity = envelope(payloads().remove(0));
    identity["payload"]["runtimeToken"] = json!("synthetic-secret");
    assert_eq!(decode(&identity), Err(ProtocolError::InvalidMessage));
}

#[test]
fn errors_never_echo_input() {
    let error = Message::decode(br#"{"privateToken":"synthetic-secret"}"#).unwrap_err();
    assert_eq!(error.to_string(), "Invalid collaboration message");
    assert_eq!(format!("{error:?}"), "InvalidMessage");
}
