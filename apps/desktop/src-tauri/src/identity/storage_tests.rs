use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;

use super::FileReplacer;
use super::IdentityStore;
use super::StorageError;
use crate::identity::types::BrainMembershipRole;
use crate::identity::types::BrainMembershipSummary;
use crate::identity::types::RivloomIdentity;

#[test]
fn missing_file_loads_as_missing() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);

    assert_eq!(store.load().unwrap(), None);
}

#[test]
fn saved_identity_round_trips_with_only_the_identity_contract() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    let identity = identity("Alice");

    store.save(&identity).unwrap();

    assert_eq!(store.load().unwrap(), Some(identity));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&store.path).unwrap()).unwrap(),
        json!({
            "version": 1,
            "identity": {
                "identityId": "identity-v1-11111111111111111111111111111111",
                "displayName": "Alice",
                "deviceId": "device-v1-22222222222222222222222222222222",
                "brainMembership": {
                    "brainId": "brain-1",
                    "memberId": "member-1",
                    "role": "member",
                },
            },
        })
    );
}

#[test]
fn invalid_json_is_quarantined_before_loading_empty() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    let private_contents = "not valid identity json";
    fs::write(&store.path, private_contents).unwrap();

    assert_eq!(store.load().unwrap(), None);
    assert!(!store.path.exists());
    let quarantined = files_with_marker(&temp_dir, ".corrupt-");
    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        fs::read_to_string(&quarantined[0]).unwrap(),
        private_contents
    );
}

#[test]
fn future_version_is_rejected_and_never_overwritten() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    let future_contents = serde_json::to_vec_pretty(&json!({
        "version": 2,
        "identity": {"futureSchema": true},
    }))
    .unwrap();
    fs::write(&store.path, &future_contents).unwrap();

    assert_eq!(store.load(), Err(StorageError::UnsupportedVersion));
    assert_eq!(
        store.save(&identity("Alice")),
        Err(StorageError::UnsupportedVersion)
    );
    assert_eq!(fs::read(&store.path).unwrap(), future_contents);
}

#[test]
fn oversized_file_is_rejected_without_being_moved_or_overwritten() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    let oversized_contents = format!(
        r#"{{"version":1,"identity":{{"displayName":"{}"}}}}"#,
        "x".repeat(16 * 1024)
    )
    .into_bytes();
    fs::write(&store.path, &oversized_contents).unwrap();

    assert_eq!(store.load(), Err(StorageError::Read));
    assert_eq!(fs::read(&store.path).unwrap(), oversized_contents);
}

#[test]
fn replacement_failure_preserves_the_previous_identity() {
    let temp_dir = tempfile::tempdir().unwrap();
    let working_store = store(&temp_dir);
    working_store.save(&identity("Alice")).unwrap();
    let old_contents = fs::read(&working_store.path).unwrap();
    let failing_store =
        IdentityStore::with_replacer(working_store.path.clone(), Arc::new(FailingReplacer));

    assert_eq!(
        failing_store.save(&identity("Bob")),
        Err(StorageError::Write)
    );
    assert_eq!(fs::read(&working_store.path).unwrap(), old_contents);
    assert_eq!(
        files_with_marker(&temp_dir, ".tmp-"),
        Vec::<std::path::PathBuf>::new()
    );
}

fn store(temp_dir: &TempDir) -> IdentityStore {
    fs::create_dir_all(temp_dir.path().join("settings")).unwrap();
    IdentityStore::new(temp_dir.path().join("settings/identity-v1.json"))
}

fn identity(display_name: &str) -> RivloomIdentity {
    RivloomIdentity {
        identity_id: "identity-v1-11111111111111111111111111111111".to_string(),
        display_name: display_name.to_string(),
        device_id: "device-v1-22222222222222222222222222222222".to_string(),
        brain_membership: Some(BrainMembershipSummary {
            brain_id: "brain-1".to_string(),
            member_id: "member-1".to_string(),
            role: BrainMembershipRole::Member,
        }),
    }
}

fn files_with_marker(temp_dir: &TempDir, marker: &str) -> Vec<std::path::PathBuf> {
    let mut paths = fs::read_dir(temp_dir.path().join("settings"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(marker))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

#[derive(Debug)]
struct FailingReplacer;

impl FileReplacer for FailingReplacer {
    fn replace(&self, _source: &Path, _destination: &Path) -> io::Result<()> {
        Err(io::Error::other("injected replacement failure"))
    }
}
