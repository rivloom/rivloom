use super::credential::{AccessError, CredentialBinding, CredentialRegistry};
use super::invitation::{InvitationRegistry, JoinRequest};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

const NOW: i64 = 1_788_000_000;

#[test]
fn restored_credentials_preserve_revocation_expiry_and_clock() {
    let mut registry = CredentialRegistry::new("brain-1".into()).unwrap();
    let issued = registry
        .issue(
            CredentialBinding {
                brain_id: "brain-1".into(),
                member_id: "member-1".into(),
                node_id: "node-1".into(),
                device_id: "device-1".into(),
            },
            NOW,
        )
        .unwrap();
    let session = registry
        .connect(&issued.binding, &issued.secret, NOW)
        .unwrap();
    registry.revoke_member("member-1").unwrap();
    let bytes = serde_json::to_vec(&registry).unwrap();
    let mut restored: CredentialRegistry = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        serde_json::to_value(&restored).unwrap(),
        serde_json::to_value(&registry).unwrap()
    );
    assert_eq!(
        restored.authorize_task(&session, NOW),
        Err(AccessError::Rejected)
    );
    assert_eq!(
        restored.connect(&issued.binding, &issued.secret, NOW - 1),
        Err(AccessError::Rejected)
    );
    let mut active = serde_json::to_value(&registry).unwrap();
    active["records"]["node-1"]["revoked"] = json!(false);
    let mut restored: CredentialRegistry = serde_json::from_value(active).unwrap();
    assert!(
        restored
            .connect(&issued.binding, &issued.secret, NOW)
            .is_ok()
    );
    assert_eq!(
        restored.authorize_task(&session, issued.expires_at),
        Err(AccessError::Rejected)
    );
}

#[test]
fn credential_restore_rejects_inconsistent_identity_time_and_revocation() {
    let mut registry = CredentialRegistry::new("brain-1".into()).unwrap();
    for node in ["node-1", "node-2"] {
        registry
            .issue(
                CredentialBinding {
                    brain_id: "brain-1".into(),
                    member_id: "member-1".into(),
                    node_id: node.into(),
                    device_id: "device-1".into(),
                },
                NOW,
            )
            .unwrap();
    }
    let original = serde_json::to_value(&registry).unwrap();
    for (pointer, invalid) in [
        ("/records/node-1/binding/brainId", json!("brain-2")),
        ("/records/node-1/binding/nodeId", json!("node-2")),
        ("/records/node-1/binding/deviceId", json!("../private")),
        ("/records/node-1/expiresAt", json!(NOW)),
        ("/records/node-1/issuedAt", json!(NOW + 1)),
        ("/records/node-1/revoked", json!(true)),
        ("/clock/high_water_at", json!(-1)),
    ] {
        let mut invalid_snapshot = original.clone();
        *invalid_snapshot.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            serde_json::from_value::<CredentialRegistry>(invalid_snapshot).is_err(),
            "{pointer}"
        );
    }
    let mut unknown = original;
    unknown["secret"] = json!("private-value");
    assert!(serde_json::from_value::<CredentialRegistry>(unknown).is_err());
}

#[test]
fn duplicate_authority_keys_and_oversize_collections_are_rejected() {
    let mut registry = CredentialRegistry::new("brain-1".into()).unwrap();
    registry
        .issue(
            CredentialBinding {
                brain_id: "brain-1".into(),
                member_id: "member-1".into(),
                node_id: "node-1".into(),
                device_id: "device-1".into(),
            },
            NOW,
        )
        .unwrap();
    let mut document = serde_json::to_value(&registry).unwrap();
    let record = document["records"]["node-1"].clone();
    let duplicate = format!(
        r#"{{"brainId":"brain-1","clock":{{"high_water_at":{NOW}}},"records":{{"node-1":{record},"node-1":{record}}}}}"#
    );
    assert!(serde_json::from_str::<CredentialRegistry>(&duplicate).is_err());
    let records = document["records"].as_object_mut().unwrap();
    for index in 0..65 {
        let key = format!("extra-{index}");
        let mut record = record.clone();
        record["binding"]["nodeId"] = json!(key);
        records.insert(key, record);
    }
    assert!(serde_json::from_value::<CredentialRegistry>(document).is_err());
}

#[test]
fn restored_invitation_is_consumed_once_and_invalid_ttl_is_rejected() {
    let mut original = InvitationRegistry::new("brain-1".into()).unwrap();
    let invitation = original.create(NOW).unwrap();
    let document = serde_json::to_value(&original).unwrap();
    let mut invalid = document.clone();
    invalid["pending"][&invitation.invitation_id]["expiresAt"] = json!(NOW + 601);
    assert!(serde_json::from_value::<InvitationRegistry>(invalid).is_err());
    let mut restored: InvitationRegistry = serde_json::from_value(document).unwrap();
    let mut credentials = CredentialRegistry::new("brain-1".into()).unwrap();
    let join = || JoinRequest {
        brain_id: "brain-1",
        invitation_id: &invitation.invitation_id,
        secret: &invitation.secret,
        identity_id: "identity-1",
        device_id: "device-1",
        display_name: "Alice",
    };
    restored.redeem(join(), &mut credentials, NOW).unwrap();
    let mut consumed: InvitationRegistry =
        serde_json::from_value(serde_json::to_value(restored).unwrap()).unwrap();
    assert_eq!(
        consumed.redeem(join(), &mut credentials, NOW).unwrap_err(),
        AccessError::Rejected
    );
    assert_eq!(
        serde_json::to_value(consumed).unwrap()["pending"],
        Value::Object(Default::default())
    );
}
