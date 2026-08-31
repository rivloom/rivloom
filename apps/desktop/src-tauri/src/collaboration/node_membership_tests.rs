use super::super::host_profile::HostProfile;
use super::super::node_registration::NodeRegistration;
use super::super::test_support::{Fixture, Memory, fixture};
use super::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tempfile::TempDir;

fn identity(name: &str) -> RivloomIdentity {
    RivloomIdentity {
        identity_id: name.into(),
        device_id: format!("{name}-device"),
        display_name: name.into(),
        brain_membership: None,
    }
}

fn owner(f: &Fixture, temp: &TempDir) -> NodeSession<Memory> {
    let session = NodeSession::new(temp.path().join("alice-node"), Memory::default()).unwrap();
    session.vault.save_new(&f.owner, f.now).unwrap();
    let profile = HostProfile {
        version: 1,
        binding: f.owner.binding.clone(),
        descriptor: f.descriptor.clone(),
        credential_expires_at: f.owner.expires_at,
    };
    session
        .connect_owner(&identity("alice"), &profile, &f.descriptor.fingerprint())
        .unwrap();
    session
}

fn join(f: &Fixture, temp: &TempDir, invitation: &InvitationDisplay) -> NodeSession<Memory> {
    let session = NodeSession::new(temp.path().join("bob-node"), Memory::default()).unwrap();
    let registration = NodeRegistration::confirmed(
        &identity("bob"),
        &f.descriptor.encode().unwrap(),
        &f.descriptor.fingerprint(),
    )
    .unwrap();
    session
        .join(
            &identity("bob"),
            &registration,
            &invitation.invitation_id,
            &invitation.secret.0,
        )
        .unwrap();
    session
}

#[test]
fn owner_invites_member_reads_minimal_directory_and_revokes_without_exposing_keys() {
    let f = fixture();
    let temp = TempDir::new().unwrap();
    let alice = owner(&f, &temp);
    let invitation = alice.invite(&identity("alice")).unwrap();
    let code = invitation.secret.0.expose_secret();
    assert_eq!(
        serde_json::to_value(&invitation).unwrap(),
        json!({"brainId":"brain-1","invitationId":invitation.invitation_id,"expiresAt":invitation.expires_at,"secret":code})
    );
    assert!(!format!("{:?}", invitation.secret).contains(code));
    let bob = join(&f, &temp, &invitation);
    alice.refresh(&identity("alice")).unwrap();
    let directory = alice.members(&identity("alice")).unwrap();
    assert_eq!(
        directory
            .entries
            .iter()
            .filter(|entry| matches!(entry, DirectoryEntry::Member { .. }))
            .count(),
        2
    );
    let json = serde_json::to_string(&directory).unwrap();
    for forbidden in [
        code,
        f.owner.secret.expose_secret(),
        "identityId",
        "deviceId",
        "announcement",
        "task",
        "goal",
        "privateKey",
    ] {
        assert!(!json.contains(forbidden));
    }
    let member = directory
        .entries
        .iter()
        .find_map(|entry| match entry {
            DirectoryEntry::Member {
                member_id,
                display_name,
                ..
            } if display_name == "bob" => Some(member_id.clone()),
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        bob.invite(&identity("bob")),
        Err(SessionError::Rejected)
    ));
    bob.connect(&identity("bob")).unwrap();
    alice.revoke(&identity("alice"), member.clone()).unwrap();
    assert!(alice.members(&identity("alice")).unwrap().entries.iter().any(|entry| matches!(entry, DirectoryEntry::Member { member_id, revoked: true, .. } if member_id == &member)));
    assert_eq!(bob.refresh(&identity("bob")), Err(SessionError::Rejected));
    assert_eq!(
        bob.members(&identity("bob")),
        Err(SessionError::Disconnected)
    );
}

#[test]
fn canceled_invitation_is_not_redeemable_and_is_absent_from_status_and_directory() {
    let f = fixture();
    let temp = TempDir::new().unwrap();
    let alice = owner(&f, &temp);
    let invitation = alice.invite(&identity("alice")).unwrap();
    alice
        .cancel_invite(&identity("alice"), invitation.invitation_id.clone())
        .unwrap();
    let bob = NodeSession::new(temp.path().join("bob-node"), Memory::default()).unwrap();
    let registration = NodeRegistration::confirmed(
        &identity("bob"),
        &f.descriptor.encode().unwrap(),
        &f.descriptor.fingerprint(),
    )
    .unwrap();
    assert_eq!(
        bob.join(
            &identity("bob"),
            &registration,
            &invitation.invitation_id,
            &invitation.secret.0
        ),
        Err(SessionError::Rejected)
    );
    for bytes in [
        serde_json::to_string(&alice.status(&identity("alice")).unwrap()).unwrap(),
        serde_json::to_string(&alice.members(&identity("alice")).unwrap()).unwrap(),
    ] {
        assert!(!bytes.contains(invitation.secret.0.expose_secret()));
        assert!(!bytes.contains(&invitation.invitation_id));
    }
}

#[test]
fn owner_registration_requires_confirmation_correct_device_and_protected_credential() {
    let mut f = fixture();
    let temp = TempDir::new().unwrap();
    let session = NodeSession::new(temp.path().join("node"), Memory::default()).unwrap();
    let profile = HostProfile {
        version: 1,
        binding: f.owner.binding.clone(),
        descriptor: f.descriptor.clone(),
        credential_expires_at: f.owner.expires_at,
    };
    assert_eq!(
        session.connect_owner(&identity("alice"), &profile, &"0".repeat(64)),
        Err(SessionError::Invalid)
    );
    assert_eq!(
        session.connect_owner(&identity("bob"), &profile, &f.descriptor.fingerprint()),
        Err(SessionError::Invalid)
    );
    assert_eq!(
        session.connect_owner(&identity("alice"), &profile, &f.descriptor.fingerprint()),
        Err(SessionError::Credential)
    );
    session.vault.save_new(&f.owner, f.now).unwrap();
    let wrong_identity = RivloomIdentity {
        identity_id: "foreign".into(),
        ..identity("alice")
    };
    assert_eq!(
        session.connect_owner(&wrong_identity, &profile, &f.descriptor.fingerprint()),
        Err(SessionError::Invalid)
    );
    assert!(!temp.path().join("node").exists());
    session
        .connect_owner(&identity("alice"), &profile, &f.descriptor.fingerprint())
        .unwrap();
    f.server.stop();
    assert_eq!(
        session.connect_owner(&identity("alice"), &profile, &f.descriptor.fingerprint()),
        Err(SessionError::Existing)
    );
}

#[test]
fn malformed_invitation_deserialization_returns_no_input_or_secret_in_errors() {
    for secret in [
        "sensitive-short".to_string(),
        "z".repeat(64),
        "x".repeat(1024),
    ] {
        let error = serde_json::from_value::<InvitationSecret>(Value::String(secret.clone()))
            .unwrap_err()
            .to_string();
        assert!(!error.contains(&secret));
        assert!(error.contains("Invalid invitation code"));
    }
}
