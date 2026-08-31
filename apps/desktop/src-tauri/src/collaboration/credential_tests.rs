use super::*;
use pretty_assertions::assert_eq;

const NOW: i64 = 1_788_000_000;

fn binding(node: &str, member: &str) -> CredentialBinding {
    CredentialBinding {
        brain_id: "brain-1".into(),
        member_id: member.into(),
        node_id: node.into(),
        device_id: format!("device-{node}"),
    }
}

#[test]
fn random_secrets_round_trip_and_never_appear_in_debug_or_registry() {
    let first = SecretToken::generate().unwrap();
    let second = SecretToken::generate().unwrap();
    assert_ne!(first.expose_secret(), second.expose_secret());
    let parsed = SecretToken::parse(first.expose_secret()).unwrap();
    assert!(parsed.matches(
        &first.digest(SecretPurpose::NodeCredential),
        SecretPurpose::NodeCredential
    ));
    assert!(!parsed.matches(
        &first.digest(SecretPurpose::Invitation),
        SecretPurpose::NodeCredential
    ));
    assert_eq!(format!("{first:?}"), "SecretToken([REDACTED])");
    for invalid in ["", "x", &"A".repeat(64), &"a".repeat(65), &"界".repeat(64)] {
        assert_eq!(
            SecretToken::parse(invalid).unwrap_err(),
            AccessError::Rejected
        );
    }
    let mut registry = CredentialRegistry::new("brain-1".into()).unwrap();
    let issued = registry.issue(binding("node-1", "member-1"), NOW).unwrap();
    let secret = issued.secret.expose_secret();
    assert!(!format!("{issued:?}").contains(secret));
    assert!(!format!("{registry:?}").contains(secret));
    assert!(!serde_json::to_string(&registry).unwrap().contains(secret));
}

#[test]
fn credential_binds_every_identity_field_and_checks_the_secret() {
    let mut registry = CredentialRegistry::new("brain-1".into()).unwrap();
    let issued = registry.issue(binding("node-1", "member-1"), NOW).unwrap();
    let session = registry
        .connect(&issued.binding, &issued.secret, NOW)
        .unwrap();
    assert_eq!(registry.authorize_task(&session, NOW), Ok(()));
    let wrong_secret = SecretToken::generate().unwrap();
    assert_eq!(
        registry.connect(&issued.binding, &wrong_secret, NOW),
        Err(AccessError::Rejected)
    );
    for field in 0..4 {
        let mut wrong = issued.binding.clone();
        match field {
            0 => wrong.brain_id = "brain-other".into(),
            1 => wrong.member_id = "member-other".into(),
            2 => wrong.node_id = "node-other".into(),
            3 => wrong.device_id = "device-other".into(),
            _ => unreachable!(),
        }
        assert_eq!(
            registry.connect(&wrong, &issued.secret, NOW),
            Err(AccessError::Rejected)
        );
    }
}

#[test]
fn revocation_rejects_reconnect_and_cached_session_tasks_for_all_member_nodes() {
    let mut registry = CredentialRegistry::new("brain-1".into()).unwrap();
    let first = registry.issue(binding("node-1", "member-1"), NOW).unwrap();
    let second = registry.issue(binding("node-2", "member-1"), NOW).unwrap();
    let other = registry.issue(binding("node-3", "member-2"), NOW).unwrap();
    let session = registry
        .connect(&first.binding, &first.secret, NOW)
        .unwrap();
    registry.revoke_member("member-1").unwrap();
    assert_eq!(
        registry.authorize_task(&session, NOW),
        Err(AccessError::Rejected)
    );
    for issued in [&first, &second] {
        assert_eq!(
            registry.connect(&issued.binding, &issued.secret, NOW),
            Err(AccessError::Rejected)
        );
    }
    assert!(registry.issue(binding("node-4", "member-1"), NOW).is_err());
    assert!(registry.connect(&other.binding, &other.secret, NOW).is_ok());
    assert_eq!(registry.revoke_member("member-1"), Ok(()));
}

#[test]
fn expiry_is_exclusive_and_clock_rollback_cannot_revive_a_session() {
    let mut registry = CredentialRegistry::new("brain-1".into()).unwrap();
    let issued = registry.issue(binding("node-1", "member-1"), NOW).unwrap();
    let session = registry
        .connect(&issued.binding, &issued.secret, issued.expires_at - 1)
        .unwrap();
    assert_eq!(
        registry.authorize_task(&session, issued.expires_at),
        Err(AccessError::Rejected)
    );
    assert_eq!(
        registry.connect(&issued.binding, &issued.secret, NOW),
        Err(AccessError::Rejected)
    );
}

#[test]
fn failed_issuance_preserves_existing_credentials_and_capacity_is_bounded() {
    let mut registry = CredentialRegistry::new("brain-1".into()).unwrap();
    let first = registry.issue(binding("node-0", "member-0"), NOW).unwrap();
    let before = serde_json::to_value(&registry).unwrap();
    assert!(registry.issue(first.binding.clone(), NOW).is_err());
    let mut invalid = binding("node-bad", "member-bad");
    invalid.device_id = "C:\\private".into();
    assert!(registry.issue(invalid, NOW).is_err());
    assert_eq!(serde_json::to_value(&registry).unwrap(), before);
    for index in 1..MAX_CREDENTIALS {
        registry
            .issue(
                binding(&format!("node-{index}"), &format!("member-{index}")),
                NOW,
            )
            .unwrap();
    }
    assert_eq!(
        registry
            .issue(binding("node-extra", "member-extra"), NOW)
            .unwrap_err(),
        AccessError::Capacity
    );
    registry.revoke_member("member-0").unwrap();
    assert_eq!(
        registry
            .issue(binding("node-extra", "member-extra"), NOW)
            .unwrap_err(),
        AccessError::Capacity
    );
}

#[test]
fn rejected_access_uses_closed_errors_without_input_details() {
    let error = SecretToken::parse("synthetic-private-value").unwrap_err();
    assert_eq!(error.to_string(), "Collaboration access rejected");
    assert_eq!(format!("{error:?}"), "Rejected");
}

#[test]
fn session_cannot_authenticate_against_another_registry_with_reused_ids() {
    let mut original = CredentialRegistry::new("brain-1".into()).unwrap();
    let issued = original.issue(binding("node-1", "member-1"), NOW).unwrap();
    let session = original
        .connect(&issued.binding, &issued.secret, NOW)
        .unwrap();
    let mut replacement = CredentialRegistry::new("brain-1".into()).unwrap();
    replacement.issue(issued.binding.clone(), NOW).unwrap();
    assert_eq!(
        replacement.authorize_task(&session, NOW),
        Err(AccessError::Rejected)
    );
    assert_eq!(
        replacement.connect(&issued.binding, &issued.secret, NOW),
        Err(AccessError::Rejected)
    );
}
