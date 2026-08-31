use std::sync::Arc;

use pretty_assertions::assert_eq;
use tempfile::TempDir;

use super::DEFAULT_DISPLAY_NAME;
use super::IdentityIdGenerator;
use super::IdentityIdKind;
use super::IdentityService;
use super::IdentityServiceError;
use crate::identity::IdentityStore;
use crate::identity::types::RivloomIdentity;

#[test]
fn first_read_creates_ids_that_remain_stable_across_reads_and_restarts() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store_path = store_path(&temp_dir);
    let service = new_service(&store_path, Arc::new(FixedIdGenerator));

    let identity = service.get().unwrap();

    assert_eq!(
        identity,
        RivloomIdentity {
            identity_id: valid_identity_id(),
            display_name: DEFAULT_DISPLAY_NAME.to_string(),
            device_id: valid_device_id(),
            brain_membership: None,
        }
    );
    assert_eq!(service.get().unwrap(), identity);
    let reloaded = new_service(&store_path, Arc::new(PanicIdGenerator));
    assert_eq!(reloaded.get().unwrap(), identity);
}

#[test]
fn display_name_collapses_whitespace_and_persists_the_normalized_value() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = store_path(&temp_dir);
    let service = new_service(&path, Arc::new(FixedIdGenerator));

    let updated = service.update_display_name("  Alice\n\t张三  ").unwrap();

    assert_eq!(updated.display_name, "Alice 张三");
    assert_eq!(
        new_service(&path, Arc::new(PanicIdGenerator))
            .get()
            .unwrap(),
        updated
    );
}

#[test]
fn display_name_enforces_a_utf8_byte_limit_without_losing_the_last_good_value() {
    let temp_dir = tempfile::tempdir().unwrap();
    let service = new_service(&store_path(&temp_dir), Arc::new(FixedIdGenerator));
    let accepted = service.update_display_name(&"a".repeat(80)).unwrap();

    assert_eq!(accepted.display_name.len(), 80);
    assert_eq!(
        service.update_display_name(&"界".repeat(27)),
        Err(IdentityServiceError::InvalidDisplayName)
    );
    assert_eq!(
        service.update_display_name(" \n\t "),
        Err(IdentityServiceError::InvalidDisplayName)
    );
    assert_eq!(service.get().unwrap(), accepted);
}

#[test]
fn invalid_stored_identity_is_rejected_without_overwriting_the_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = store_path(&temp_dir);
    let store = IdentityStore::new(path.clone());
    store
        .save(&RivloomIdentity {
            identity_id: "not-a-valid-id".to_string(),
            display_name: " Alice ".to_string(),
            device_id: valid_device_id(),
            brain_membership: None,
        })
        .unwrap();
    let contents = std::fs::read(&path).unwrap();

    assert_eq!(
        new_service(&path, Arc::new(PanicIdGenerator)).get(),
        Err(IdentityServiceError::InvalidStoredIdentity)
    );
    assert_eq!(std::fs::read(path).unwrap(), contents);
}

fn new_service(path: &std::path::Path, generator: Arc<dyn IdentityIdGenerator>) -> IdentityService {
    IdentityService::with_generator(IdentityStore::new(path.to_path_buf()), generator)
}

fn store_path(temp_dir: &TempDir) -> std::path::PathBuf {
    temp_dir.path().join("settings/identity-v1.json")
}

fn valid_identity_id() -> String {
    "identity-v1-11111111111111111111111111111111".to_string()
}

fn valid_device_id() -> String {
    "device-v1-22222222222222222222222222222222".to_string()
}

#[derive(Debug)]
struct FixedIdGenerator;

impl IdentityIdGenerator for FixedIdGenerator {
    fn generate(&self, kind: IdentityIdKind) -> String {
        match kind {
            IdentityIdKind::Identity => valid_identity_id(),
            IdentityIdKind::Device => valid_device_id(),
        }
    }
}

#[derive(Debug)]
struct PanicIdGenerator;

impl IdentityIdGenerator for PanicIdGenerator {
    fn generate(&self, _kind: IdentityIdKind) -> String {
        panic!("stored identities must not generate replacement IDs")
    }
}
