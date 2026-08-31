use super::super::credential::MAX_CREDENTIALS;
use super::*;
use pretty_assertions::assert_eq;

const NOW: i64 = 1_788_000_000;

fn request(invitation: &IssuedInvitation) -> JoinRequest<'_> {
    JoinRequest {
        brain_id: &invitation.brain_id,
        invitation_id: &invitation.invitation_id,
        secret: &invitation.secret,
        identity_id: "local-identity",
        device_id: "local-device",
        display_name: "Alice",
    }
}

#[test]
fn redeem_once_creates_member_and_independent_node_credential() {
    let mut invitations = InvitationRegistry::new("brain-1".into()).unwrap();
    let mut credentials = CredentialRegistry::new("brain-1".into()).unwrap();
    let invitation = invitations.create(NOW).unwrap();
    let joined = invitations
        .redeem(request(&invitation), &mut credentials, NOW)
        .unwrap();
    assert_eq!(
        joined.member,
        JoinedMember {
            member_id: joined.credential.binding.member_id.clone(),
            identity_id: "local-identity".into(),
            display_name: "Alice".into(),
            role: JoinedRole::Member,
        }
    );
    assert_eq!(joined.credential.binding.brain_id, "brain-1");
    assert_eq!(joined.credential.binding.device_id, "local-device");
    assert_ne!(
        joined.credential.secret.expose_secret(),
        invitation.secret.expose_secret()
    );
    let session = credentials
        .connect(&joined.credential.binding, &joined.credential.secret, NOW)
        .unwrap();
    assert_eq!(credentials.authorize_task(&session, NOW), Ok(()));
    let before = serde_json::to_value(&credentials).unwrap();
    assert_eq!(
        invitations
            .redeem(request(&invitation), &mut credentials, NOW)
            .unwrap_err(),
        AccessError::Rejected
    );
    assert_eq!(serde_json::to_value(&credentials).unwrap(), before);
    assert_eq!(
        credentials.connect(&joined.credential.binding, &invitation.secret, NOW),
        Err(AccessError::Rejected)
    );
}

#[test]
fn wrong_proof_or_brain_does_not_consume_invitation_or_issue_credentials() {
    let mut invitations = InvitationRegistry::new("brain-1".into()).unwrap();
    let mut credentials = CredentialRegistry::new("brain-1".into()).unwrap();
    let invitation = invitations.create(NOW).unwrap();
    let bad_secret = SecretToken::generate().unwrap();
    let before = serde_json::to_value(&credentials).unwrap();
    let mut wrong = request(&invitation);
    wrong.secret = &bad_secret;
    assert_eq!(
        invitations
            .redeem(wrong, &mut credentials, NOW)
            .unwrap_err(),
        AccessError::Rejected
    );
    let mut wrong = request(&invitation);
    wrong.brain_id = "brain-2";
    assert_eq!(
        invitations
            .redeem(wrong, &mut credentials, NOW)
            .unwrap_err(),
        AccessError::Rejected
    );
    assert_eq!(serde_json::to_value(&credentials).unwrap(), before);
    let pending_before = serde_json::to_value(&invitations).unwrap();
    let mut other_brain = CredentialRegistry::new("brain-2".into()).unwrap();
    assert_eq!(
        invitations
            .redeem(request(&invitation), &mut other_brain, NOW)
            .unwrap_err(),
        AccessError::Rejected
    );
    assert_eq!(serde_json::to_value(&invitations).unwrap(), pending_before);
    assert!(
        invitations
            .redeem(request(&invitation), &mut credentials, NOW)
            .is_ok()
    );
}

#[test]
fn expiry_is_exclusive_and_rollback_or_cancellation_cannot_revive_an_invitation() {
    let mut invitations = InvitationRegistry::new("brain-1".into()).unwrap();
    let mut credentials = CredentialRegistry::new("brain-1".into()).unwrap();
    let invitation = invitations.create(NOW).unwrap();
    assert_eq!(
        invitations
            .redeem(
                request(&invitation),
                &mut credentials,
                invitation.expires_at
            )
            .unwrap_err(),
        AccessError::Rejected
    );
    assert_eq!(
        invitations
            .redeem(request(&invitation), &mut credentials, NOW)
            .unwrap_err(),
        AccessError::Rejected
    );
    let fresh = invitations.create(invitation.expires_at).unwrap();
    assert!(
        invitations
            .redeem(request(&fresh), &mut credentials, fresh.expires_at - 1)
            .is_ok()
    );
    let cancelled = invitations.create(fresh.expires_at).unwrap();
    invitations.cancel(&cancelled.invitation_id).unwrap();
    assert_eq!(
        invitations
            .redeem(request(&cancelled), &mut credentials, fresh.expires_at)
            .unwrap_err(),
        AccessError::Rejected
    );
}

#[test]
fn malformed_join_keeps_invitation_usable_without_echoing_secret() {
    let mut invitations = InvitationRegistry::new("brain-1".into()).unwrap();
    let mut credentials = CredentialRegistry::new("brain-1".into()).unwrap();
    let invitation = invitations.create(NOW).unwrap();
    let before = serde_json::to_value(&invitations).unwrap();
    let secret = invitation.secret.expose_secret();
    assert!(!before.to_string().contains(secret));
    assert!(!format!("{invitations:?}").contains(secret));
    for (identity, device, name) in [
        ("../identity", "device-1", "Alice"),
        ("identity-1", "C:/private", "Alice"),
        ("identity-1", "device-1", ""),
        ("identity-1", "device-1", " \n"),
        ("identity-1", "device-1", &"界".repeat(86)),
        ("identity-1", "device-1", "Alice\0"),
    ] {
        let mut invalid = request(&invitation);
        invalid.identity_id = identity;
        invalid.device_id = device;
        invalid.display_name = name;
        let error = invitations
            .redeem(invalid, &mut credentials, NOW)
            .unwrap_err();
        assert_eq!(error, AccessError::Rejected);
        assert_eq!(error.to_string(), "Collaboration access rejected");
    }
    assert_eq!(serde_json::to_value(&invitations).unwrap(), before);
    assert!(
        invitations
            .redeem(request(&invitation), &mut credentials, NOW)
            .is_ok()
    );
    let secret = invitation.secret.expose_secret();
    assert!(!format!("{invitation:?} {invitations:?} {credentials:?}").contains(secret));
    assert!(
        !serde_json::to_string(&invitations)
            .unwrap()
            .contains(secret)
    );
}

#[test]
fn pending_capacity_recovers_after_expiry_and_ids_are_not_client_selected() {
    let mut invitations = InvitationRegistry::new("brain-1".into()).unwrap();
    let first = invitations.create(NOW).unwrap();
    for _ in 1..MAX_PENDING_INVITATIONS {
        invitations.create(NOW).unwrap();
    }
    assert_eq!(invitations.create(NOW).unwrap_err(), AccessError::Capacity);
    let fresh = invitations.create(first.expires_at).unwrap();
    assert_ne!(fresh.invitation_id, first.invitation_id);
    let mut credentials = CredentialRegistry::new("brain-1".into()).unwrap();
    assert_eq!(
        invitations
            .redeem(request(&first), &mut credentials, first.expires_at)
            .unwrap_err(),
        AccessError::Rejected
    );
    assert!(
        invitations
            .redeem(request(&fresh), &mut credentials, first.expires_at)
            .is_ok()
    );
}

#[test]
fn full_credential_registry_does_not_spend_invitation() {
    let mut invitations = InvitationRegistry::new("brain-1".into()).unwrap();
    let invitation = invitations.create(NOW).unwrap();
    let mut credentials = CredentialRegistry::new("brain-1".into()).unwrap();
    for index in 0..MAX_CREDENTIALS {
        credentials
            .issue(
                CredentialBinding {
                    brain_id: "brain-1".into(),
                    member_id: format!("member-{index}"),
                    node_id: format!("node-{index}"),
                    device_id: format!("device-{index}"),
                },
                NOW,
            )
            .unwrap();
    }
    let before = serde_json::to_value(&invitations).unwrap();
    let before_credentials = serde_json::to_value(&credentials).unwrap();
    assert_eq!(
        invitations
            .redeem(request(&invitation), &mut credentials, NOW)
            .unwrap_err(),
        AccessError::Capacity
    );
    assert_eq!(serde_json::to_value(&invitations).unwrap(), before);
    assert_eq!(
        serde_json::to_value(&credentials).unwrap(),
        before_credentials
    );
}

#[test]
fn revocation_after_join_blocks_work_and_a_new_invite_never_reuses_membership() {
    let mut invitations = InvitationRegistry::new("brain-1".into()).unwrap();
    let mut credentials = CredentialRegistry::new("brain-1".into()).unwrap();
    let first = invitations.create(NOW).unwrap();
    let old = invitations
        .redeem(request(&first), &mut credentials, NOW)
        .unwrap();
    let session = credentials
        .connect(&old.credential.binding, &old.credential.secret, NOW)
        .unwrap();
    credentials.revoke_member(&old.member.member_id).unwrap();
    assert_eq!(
        credentials.authorize_task(&session, NOW),
        Err(AccessError::Rejected)
    );
    assert_eq!(
        credentials.connect(&old.credential.binding, &old.credential.secret, NOW),
        Err(AccessError::Rejected)
    );
    let second = invitations.create(NOW).unwrap();
    let new = invitations
        .redeem(request(&second), &mut credentials, NOW)
        .unwrap();
    assert_ne!(old.member.member_id, new.member.member_id);
    assert_ne!(
        old.credential.binding.node_id,
        new.credential.binding.node_id
    );
    assert_eq!(
        credentials.authorize_task(&session, NOW),
        Err(AccessError::Rejected)
    );
    assert!(
        credentials
            .connect(&new.credential.binding, &new.credential.secret, NOW)
            .is_ok()
    );
}
