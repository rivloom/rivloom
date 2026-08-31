use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::*;

fn artifact() -> Value {
    json!({
        "artifactId": "artifact-1", "taskId": "task-1", "runId": "run-1",
        "baselineCommit": "a".repeat(40), "state": "empty", "limitBytes": 524288,
        "byteCount": 0,
        "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    })
}

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
        json!({"type": "artifact", "data": artifact()}),
        json!({"type": "runReceipt", "data": {
            "content": {
                "taskId": "task-1", "runId": "run-1", "nodeId": "node-bob",
                "runtimeId": "codex", "runtimeVersion": "1.2.3",
                "startedAt": 1788000000, "finishedAt": 1788000090, "outcome": "success",
                "summary": "Requested change is ready.", "failure": null,
                "tests": {"state": "notReported"}, "artifact": artifact()
            },
            "contentSha256": "b22ed69ff79dccb1da4985a875c30e7aae06fa5aa1c0d46b2538e66ffd56fe40"
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

// Rehash mutated fixtures so semantic failures cannot hide behind a stale hash.
fn reseal(value: &mut Value) {
    if let Ok(content) =
        serde_json::from_value::<ReceiptContent>(value["payload"]["data"]["content"].clone())
    {
        value["payload"]["data"]["contentSha256"] = json!(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&content).unwrap())
        ));
    }
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
fn artifact_and_receipt_invariants_reject_forged_results() {
    for (field, value) in [
        ("baselineCommit", json!("main")),
        ("sha256", json!("not-a-hash")),
        ("state", json!("complete")),
        ("limitBytes", json!(524289)),
        ("byteCount", json!(1)),
        ("body", json!("private patch")),
    ] {
        let mut invalid = envelope(payloads().remove(4));
        invalid["payload"]["data"][field] = value;
        assert_eq!(decode(&invalid), Err(ProtocolError::InvalidMessage));
    }
    let mut tampered = envelope(payloads().remove(5));
    tampered["payload"]["data"]["content"]["summary"] = json!("Tampered");
    assert_eq!(decode(&tampered), Err(ProtocolError::InvalidMessage));
    for (field, value) in [
        ("summary", json!("x".repeat(4097))),
        ("finishedAt", json!(0)),
        ("outcome", json!("failed")),
        ("failure", json!("raw private error")),
        (
            "tests",
            json!({"state": "reported", "executions": vec![json!({"name":"test","exitCode":0}); 33]}),
        ),
        ("logs", json!(["raw runtime output"])),
    ] {
        let mut invalid = envelope(payloads().remove(5));
        invalid["payload"]["data"]["content"][field] = value;
        reseal(&mut invalid);
        assert_eq!(decode(&invalid), Err(ProtocolError::InvalidMessage));
    }
    let mut invalid = envelope(payloads().remove(5));
    invalid["payload"]["data"]["content"]["artifact"]["runId"] = json!("other-run");
    reseal(&mut invalid);
    assert_eq!(decode(&invalid), Err(ProtocolError::InvalidMessage));
}

#[test]
fn terminal_receipts_preserve_unknown_and_unreported_results() {
    for (outcome, failure) in [
        ("cancelled", Value::Null),
        ("failed", json!("executionFailed")),
        ("outcomeUnknown", json!("connectionLost")),
    ] {
        let mut expected = envelope(payloads().remove(5));
        expected["payload"]["data"]["content"]["outcome"] = json!(outcome);
        expected["payload"]["data"]["content"]["failure"] = failure;
        reseal(&mut expected);
        let encoded = decode(&expected).unwrap().encode().unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), expected);
    }
}

#[test]
fn unit_variants_and_tag_wrappers_reject_unrecognized_fields() {
    let mut assignment = envelope(payloads().remove(3));
    assignment["payload"]["data"]["decision"] = json!({"state": "offered", "networkAccess": true});
    assert_eq!(decode(&assignment), Err(ProtocolError::InvalidMessage));
    let mut receipt = envelope(payloads().remove(5));
    receipt["payload"]["data"]["content"]["tests"] = json!({"state": "notReported", "logs": []});
    assert_eq!(decode(&receipt), Err(ProtocolError::InvalidMessage));
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

#[test]
fn string_enums_do_not_accept_object_aliases() {
    for (index, field, variant) in [
        (0, "role", "member"),
        (1, "runtimeId", "codex"),
        (2, "status", "offered"),
        (2, "expectedArtifact", "patch"),
        (3, "executionPolicy", "managedWorktreeOffline"),
        (4, "state", "empty"),
    ] {
        let mut invalid = envelope(payloads().remove(index));
        invalid["payload"]["data"][field] = json!({(variant): null});
        assert_eq!(decode(&invalid), Err(ProtocolError::InvalidMessage));
    }
    let mut node = envelope(payloads().remove(1));
    node["payload"]["data"]["capabilities"] = json!([{"taskRun": null}]);
    assert_eq!(decode(&node), Err(ProtocolError::InvalidMessage));
}

#[test]
fn sending_checks_encoded_size_after_json_escaping() {
    let mut task = envelope(payloads().remove(2));
    task["payload"]["data"]["goal"] = json!("\u{0000}".repeat(4096));
    task["payload"]["data"]["constraints"] = json!(vec!["\u{0000}".repeat(1024); 8]);
    let raw: Envelope = serde_json::from_value(task).unwrap();
    assert_eq!(Message::try_from(raw), Err(ProtocolError::MessageTooLarge));
}

#[test]
fn artifact_availability_and_test_report_limits_are_preserved() {
    for (state, byte_count, sha256) in [
        ("complete", json!(524288), json!("b".repeat(64))),
        ("unsupportedEncoding", json!(1), json!("b".repeat(64))),
        ("tooLarge", Value::Null, Value::Null),
    ] {
        let mut expected = envelope(payloads().remove(4));
        expected["payload"]["data"]["state"] = json!(state);
        expected["payload"]["data"]["byteCount"] = byte_count;
        expected["payload"]["data"]["sha256"] = sha256;
        let encoded = decode(&expected).unwrap().encode().unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), expected);
    }
    let mut receipt = envelope(payloads().remove(5));
    receipt["payload"]["data"]["content"]["tests"] = json!({"state": "reported", "executions": vec![json!({"name": "x".repeat(256), "exitCode": 0}); 16]});
    reseal(&mut receipt);
    assert!(decode(&receipt).is_ok());
    for executions in [
        vec![json!({"name": "x".repeat(257), "exitCode": 0})],
        vec![json!({"name": "x".repeat(256), "exitCode": 0}); 17],
    ] {
        receipt["payload"]["data"]["content"]["tests"]["executions"] = json!(executions);
        reseal(&mut receipt);
        assert_eq!(decode(&receipt), Err(ProtocolError::InvalidMessage));
    }
}
