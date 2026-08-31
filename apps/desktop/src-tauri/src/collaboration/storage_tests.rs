use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tempfile::TempDir;

use super::super::credential::AccessError;
use super::super::invitation::{IssuedInvitation, JoinRequest};
use super::super::protocol::Message;
use super::*;

const NOW: i64 = 1_788_000_000;

fn create(directory: &Path) -> (BrainStore, IssuedCredential) {
    BrainStore::create(
        directory.to_owned(),
        "brain-1".into(),
        OwnerProfile {
            identity_id: "owner",
            device_id: "device-1",
            display_name: "Owner",
        },
        NOW,
    )
    .unwrap()
}

fn request(invitation: &IssuedInvitation) -> JoinRequest<'_> {
    JoinRequest {
        brain_id: "brain-1",
        invitation_id: &invitation.invitation_id,
        secret: &invitation.secret,
        identity_id: "member",
        device_id: "device-2",
        display_name: "Member",
    }
}

#[test]
fn single_writer_lock_is_released_on_drop_without_unlinking_its_marker() {
    let dir = TempDir::new().unwrap();
    let (store, _) = create(dir.path());
    assert_eq!(
        BrainStore::open(dir.path().into(), "brain-1", NOW).err(),
        Some(StorageError::Locked)
    );
    assert!(dir.path().join("brain-v1.lock").is_file());
    drop(store);
    let reopened = BrainStore::open(dir.path().into(), "brain-1", NOW).unwrap();
    assert_eq!(reopened.brain().unwrap().revision(), 1);
    assert!(dir.path().join("brain-v1.lock").is_file());
}

#[test]
fn committed_join_consumption_and_revocation_survive_reopen_without_plaintext_secrets() {
    let dir = TempDir::new().unwrap();
    let (mut store, owner) = create(dir.path());
    let invite = store
        .transact(NOW, |brain| brain.create_invitation(NOW))
        .unwrap();
    let joined = store
        .transact(NOW, |brain| brain.join(request(&invite), NOW))
        .unwrap();
    let session = store
        .transact(NOW, |brain| {
            brain.connect(&joined.credential.binding, &joined.credential.secret, NOW)
        })
        .unwrap();
    store
        .transact(NOW, |brain| {
            brain.revoke_member(&joined.member.member_id, brain.revision(), NOW)
        })
        .unwrap();
    let bytes = fs::read(&store.path).unwrap();
    for secret in [
        owner.secret.expose_secret(),
        invite.secret.expose_secret(),
        joined.credential.secret.expose_secret(),
    ] {
        assert!(!String::from_utf8_lossy(&bytes).contains(secret));
    }
    drop(store);
    let mut store = BrainStore::open(dir.path().into(), "brain-1", NOW).unwrap();
    assert_eq!(
        store
            .transact(NOW, |brain| brain.join(request(&invite), NOW))
            .unwrap_err(),
        StorageError::State(BrainError::Access(AccessError::Rejected))
    );
    assert_eq!(
        store.transact(NOW, |brain| brain.heartbeat(
            &session,
            brain.revision(),
            NOW
        )),
        Err(StorageError::State(BrainError::Access(
            AccessError::Rejected
        )))
    );
}

struct BeforeReplaceFailure;
impl FileReplacer for BeforeReplaceFailure {
    fn replace(&self, _source: &Path, _destination: &Path) -> io::Result<()> {
        Err(io::Error::other("synthetic private diagnostic"))
    }
}

struct AfterReplaceFailure;
impl FileReplacer for AfterReplaceFailure {
    fn replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        PlatformFileReplacer.replace(source, destination)?;
        Err(io::Error::other("synthetic private diagnostic"))
    }
}

#[test]
fn failed_commit_does_not_return_credentials_or_publish_partial_join() {
    let dir = TempDir::new().unwrap();
    let (mut store, _) = create(dir.path());
    let invite = store
        .transact(NOW, |brain| brain.create_invitation(NOW))
        .unwrap();
    let before = fs::read(&store.path).unwrap();
    store.replacer = Arc::new(BeforeReplaceFailure);
    assert_eq!(
        store
            .transact(NOW, |brain| brain.join(request(&invite), NOW))
            .unwrap_err(),
        StorageError::Write
    );
    assert_eq!(fs::read(&store.path).unwrap(), before);
    assert_eq!(
        store.brain().unwrap_err().to_string(),
        "Brain storage must be reopened after a failed commit"
    );
    drop(store);
    let mut reopened = BrainStore::open(dir.path().into(), "brain-1", NOW).unwrap();
    reopened
        .transact(NOW, |brain| brain.join(request(&invite), NOW))
        .unwrap();
    assert_eq!(reopened.brain().unwrap().state.members.len(), 2);
}

#[test]
fn uncertain_commit_is_not_retried_from_stale_memory() {
    let dir = TempDir::new().unwrap();
    let (mut store, _) = create(dir.path());
    let invite = store
        .transact(NOW, |brain| brain.create_invitation(NOW))
        .unwrap();
    store.replacer = Arc::new(AfterReplaceFailure);
    assert_eq!(
        store
            .transact(NOW, |brain| brain.join(request(&invite), NOW))
            .unwrap_err(),
        StorageError::Write
    );
    assert_eq!(
        store
            .transact(NOW, |brain| brain.create_invitation(NOW))
            .unwrap_err(),
        StorageError::Unavailable
    );
    drop(store);
    let mut reopened = BrainStore::open(dir.path().into(), "brain-1", NOW).unwrap();
    assert_eq!(reopened.brain().unwrap().state.members.len(), 2);
    assert!(
        reopened
            .transact(NOW, |brain| brain.join(request(&invite), NOW))
            .is_err()
    );
}

#[test]
fn rejected_access_persists_time_and_cannot_be_revived_by_clock_rollback() {
    let dir = TempDir::new().unwrap();
    let (mut store, owner) = create(dir.path());
    assert_eq!(
        store.transact(owner.expires_at, |brain| brain.connect(
            &owner.binding,
            &owner.secret,
            owner.expires_at
        )),
        Err(StorageError::State(BrainError::Access(
            AccessError::Rejected
        )))
    );
    drop(store);
    assert!(BrainStore::open(dir.path().into(), "brain-1", owner.expires_at - 1).is_err());
    let mut reopened = BrainStore::open(dir.path().into(), "brain-1", owner.expires_at).unwrap();
    assert!(
        reopened
            .transact(NOW, |brain| brain.connect(
                &owner.binding,
                &owner.secret,
                NOW
            ))
            .is_err()
    );
}

#[test]
fn corrupt_oversized_unknown_or_missing_storage_is_never_reinitialized() {
    let dir = TempDir::new().unwrap();
    let (store, _) = create(dir.path());
    let path = store.path.clone();
    let original = fs::read(&path).unwrap();
    drop(store);
    let mut future: Value = serde_json::from_slice(&original).unwrap();
    future["version"] = json!(2);
    let mut checksum: Value = serde_json::from_slice(&original).unwrap();
    checksum["sha256"] = json!(vec![0; 32]);
    let mut invalid_state: Document = serde_json::from_slice(&original).unwrap();
    let mut payload: Value = serde_json::from_str(&invalid_state.payload).unwrap();
    payload["nodes"] = json!({});
    invalid_state.payload = serde_json::to_string(&payload).unwrap();
    invalid_state.sha256 = Sha256::digest(invalid_state.payload.as_bytes()).into();
    for bad in [
        b"{".to_vec(),
        serde_json::to_vec(&future).unwrap(),
        serde_json::to_vec(&checksum).unwrap(),
        serde_json::to_vec(&invalid_state).unwrap(),
        vec![b' '; MAX_FILE_BYTES + 1],
    ] {
        fs::write(&path, &bad).unwrap();
        assert!(BrainStore::open(dir.path().into(), "brain-1", NOW).is_err());
        assert_eq!(fs::read(&path).unwrap(), bad);
    }
    fs::remove_file(&path).unwrap();
    assert!(BrainStore::open(dir.path().into(), "brain-1", NOW).is_err());
    assert_eq!(
        BrainStore::create(
            dir.path().into(),
            "brain-1".into(),
            OwnerProfile {
                identity_id: "owner",
                device_id: "device-1",
                display_name: "Owner",
            },
            NOW
        )
        .err(),
        Some(StorageError::Existing)
    );
}

#[test]
fn rejected_transaction_rolls_back_business_changes_but_commits_observed_time() {
    let dir = TempDir::new().unwrap();
    let (mut store, _) = create(dir.path());
    let mut expected = serde_json::to_value(store.brain().unwrap()).unwrap();
    expected["clock"]["high_water_at"] = json!(NOW + 1);
    let result = store.transact(NOW + 1, |brain| {
        brain.create_invitation(NOW + 1)?;
        Err::<(), _>(BrainError::Conflict)
    });
    assert_eq!(result, Err(StorageError::State(BrainError::Conflict)));
    let invalid = store.transact(NOW + 2, |brain| {
        brain.state.members.clear();
        Ok(())
    });
    assert_eq!(invalid, Err(StorageError::State(BrainError::Invalid)));
    expected["clock"]["high_water_at"] = json!(NOW + 2);
    assert_eq!(
        serde_json::to_value(store.brain().unwrap()).unwrap(),
        expected
    );
    drop(store);
    let reopened = BrainStore::open(dir.path().into(), "brain-1", NOW + 2).unwrap();
    assert_eq!(
        serde_json::to_value(reopened.brain().unwrap()).unwrap(),
        expected
    );
}

#[test]
fn restart_clears_presence_but_retains_the_replay_result() {
    let dir = TempDir::new().unwrap();
    let (mut store, owner) = create(dir.path());
    let session = store
        .transact(NOW, |brain| {
            brain.connect(&owner.binding, &owner.secret, NOW)
        })
        .unwrap();
    let message = Message::decode(&serde_json::to_vec(&json!({
        "protocolVersion":1,"messageId":"heartbeat","idempotencyKey":"heartbeat","brainId":"brain-1",
        "senderNodeId":owner.binding.node_id,"sentAt":NOW,"revision":store.brain().unwrap().revision(),
        "payload":{"type":"node","data":{"nodeId":owner.binding.node_id,"memberId":owner.binding.member_id,
            "deviceId":owner.binding.device_id,"runtimeId":"codex","runtimeVersion":"test","capabilities":[]}}
    })).unwrap()).unwrap();
    assert_eq!(
        store.transact(NOW, |brain| brain.apply(&session, message.clone(), NOW)),
        Ok(2)
    );
    drop(store);
    let mut reopened = BrainStore::open(dir.path().into(), "brain-1", NOW).unwrap();
    assert_eq!(reopened.brain().unwrap().revision(), 3);
    assert!(!reopened.brain().unwrap().state.nodes[&owner.binding.node_id].online);
    assert_eq!(
        reopened.transact(NOW, |brain| brain.apply(&session, message, NOW)),
        Ok(2)
    );
    assert_eq!(reopened.brain().unwrap().revision(), 3);
}

#[test]
fn external_file_changes_are_preserved_and_block_further_mutation() {
    let dir = TempDir::new().unwrap();
    let (mut store, _) = create(dir.path());
    fs::write(&store.path, b"unexpected private content").unwrap();
    assert_eq!(
        store
            .transact(NOW, |brain| brain.create_invitation(NOW))
            .unwrap_err(),
        StorageError::Changed
    );
    assert_eq!(
        fs::read(&store.path).unwrap(),
        b"unexpected private content"
    );
    assert_eq!(
        store
            .transact(NOW, |brain| brain.create_invitation(NOW))
            .unwrap_err(),
        StorageError::Unavailable
    );
}
