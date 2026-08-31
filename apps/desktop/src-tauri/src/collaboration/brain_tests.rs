use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

const NOW: i64 = 1_788_000_000;

fn bootstrap() -> (Brain, IssuedCredential) {
    Brain::bootstrap(
        "brain-1".into(),
        OwnerProfile {
            identity_id: "owner-identity",
            device_id: "owner-device",
            display_name: "Owner",
        },
        NOW,
    )
    .unwrap()
}

fn join(brain: &mut Brain) -> Enrollment {
    let invitation = brain.create_invitation(NOW).unwrap();
    brain
        .join(
            JoinRequest {
                brain_id: &invitation.brain_id,
                invitation_id: &invitation.invitation_id,
                secret: &invitation.secret,
                identity_id: "member-identity",
                device_id: "member-device",
                display_name: "Member",
            },
            NOW,
        )
        .unwrap()
}

#[test]
fn membership_presence_and_revocation_share_one_revision_authority() {
    let (mut brain, owner) = bootstrap();
    let joined = join(&mut brain);
    assert_eq!(brain.revision(), 3);
    let session = brain
        .connect(&joined.credential.binding, &joined.credential.secret, NOW)
        .unwrap();
    assert_eq!(
        brain.heartbeat(&session, /*expected_revision*/ 3, NOW),
        Ok(4)
    );
    assert_eq!(
        brain.heartbeat(&session, /*expected_revision*/ 3, NOW),
        Err(BrainError::Conflict)
    );
    assert_eq!(
        brain.revoke_member(&joined.member.member_id, /*expected_revision*/ 4, NOW),
        Ok(5)
    );
    assert_eq!(
        brain.revoke_member(&joined.member.member_id, /*expected_revision*/ 5, NOW),
        Ok(5)
    );
    assert_eq!(
        brain.heartbeat(&session, /*expected_revision*/ 5, NOW),
        Err(BrainError::Access(AccessError::Rejected))
    );
    assert!(!brain.state.nodes[&joined.credential.binding.node_id].online);
    assert!(brain.connect(&owner.binding, &owner.secret, NOW).is_ok());
    assert!(
        brain
            .revoke_member(&owner.binding.member_id, /*expected_revision*/ 5, NOW)
            .is_err()
    );
    assert_eq!(brain.revision(), 5);
}

#[test]
fn heartbeats_expire_at_the_boundary_and_restart_clears_presence() {
    let (mut brain, owner) = bootstrap();
    let session = brain.connect(&owner.binding, &owner.secret, NOW).unwrap();
    brain
        .heartbeat(&session, /*expected_revision*/ 1, NOW)
        .unwrap();
    assert_eq!(
        brain.reset_presence(NOW + 29, PresenceReset::Expired),
        Ok(2)
    );
    assert_eq!(
        brain.reset_presence(NOW + 30, PresenceReset::Expired),
        Ok(3)
    );
    assert!(!brain.state.nodes[&owner.binding.node_id].online);
    brain
        .heartbeat(&session, /*expected_revision*/ 3, NOW + 30)
        .unwrap();
    let bytes = brain.encode().unwrap();
    let mut restored = Brain::decode(&bytes, "brain-1").unwrap();
    assert_eq!(
        restored.reset_presence(NOW + 30, PresenceReset::Restart),
        Ok(5)
    );
    assert!(!restored.state.nodes[&owner.binding.node_id].online);
    assert!(
        restored
            .heartbeat(&session, /*expected_revision*/ 5, NOW)
            .is_err()
    );
}

#[test]
fn complete_snapshot_preserves_members_and_consumed_invites_without_secrets() {
    let (mut brain, owner) = bootstrap();
    let joined = join(&mut brain);
    brain
        .revoke_member(&joined.member.member_id, /*expected_revision*/ 3, NOW)
        .unwrap();
    let bytes = brain.encode().unwrap();
    for secret in [
        owner.secret.expose_secret(),
        joined.credential.secret.expose_secret(),
    ] {
        assert!(!String::from_utf8_lossy(&bytes).contains(secret));
    }
    let restored = Brain::decode(&bytes, "brain-1").unwrap();
    assert_eq!(restored.encode().unwrap(), bytes);
    assert!(restored.state.members[&joined.member.member_id].revoked);
    assert!(Brain::decode(&bytes, "brain-other").is_err());
}

#[test]
fn incomplete_or_inconsistent_snapshots_cannot_create_authority() {
    let (brain, owner) = bootstrap();
    let original = serde_json::to_value(&brain).unwrap();
    for (path, value) in [
        ("/version".to_owned(), json!(2)),
        ("/revision".to_owned(), json!(0)),
        ("/ownerMemberId".to_owned(), json!("missing")),
        ("/clock/high_water_at".to_owned(), json!(NOW - 1)),
        (
            format!("/nodes/{}/memberId", owner.binding.node_id),
            json!("missing"),
        ),
        (
            format!("/members/{}/revoked", owner.binding.member_id),
            json!(true),
        ),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(&path).unwrap() = value;
        assert!(
            Brain::decode(&serde_json::to_vec(&changed).unwrap(), "brain-1").is_err(),
            "{path}"
        );
    }
    let mut missing = original;
    missing["nodes"] = json!({});
    assert!(Brain::decode(&serde_json::to_vec(&missing).unwrap(), "brain-1").is_err());
    assert!(Brain::decode(b"{", "brain-1").is_err());
    assert!(Brain::decode(&vec![b' '; MAX_BRAIN_BYTES + 1], "brain-1").is_err());
}

#[test]
fn revision_exhaustion_fails_before_an_invitation_or_presence_mutation() {
    let (mut brain, owner) = bootstrap();
    let session = brain.connect(&owner.binding, &owner.secret, NOW).unwrap();
    brain.state.revision = MAX_REVISION;
    let before = brain.encode().unwrap();
    assert_eq!(
        brain.create_invitation(NOW).unwrap_err(),
        BrainError::Capacity
    );
    assert_eq!(
        brain.heartbeat(&session, MAX_REVISION, NOW),
        Err(BrainError::Capacity)
    );
    assert_eq!(brain.encode().unwrap(), before);
}
