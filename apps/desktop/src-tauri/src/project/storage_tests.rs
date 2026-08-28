use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;

use super::FileReplacer;
use super::PlatformFileReplacer;
use super::RecentProjectStore;
use super::StorageError;
use super::StoredProject;

#[test]
fn missing_file_loads_as_empty() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);

    assert_eq!(store.load().unwrap(), Vec::<StoredProject>::new());
}

#[test]
fn valid_file_ignores_unknown_fields() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    fs::write(
        &store.path,
        serde_json::to_vec_pretty(&json!({
            "version": 1,
            "unknown": "ignored",
            "projects": [{
                "id": "project-1",
                "path": "C:\\work\\one",
                "name": "one",
                "lastOpenedAt": 100,
                "unknown": true,
            }],
        }))
        .unwrap(),
    )
    .unwrap();

    assert_eq!(
        store.load().unwrap(),
        vec![StoredProject {
            id: "project-1".to_string(),
            path: r"C:\work\one".to_string(),
            name: "one".to_string(),
            last_opened_at: 100,
        }]
    );
}

#[test]
fn save_sorts_deduplicates_and_truncates_projects() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    let mut projects = (0..22)
        .map(|index| project(&format!("project-{index}"), index))
        .collect::<Vec<_>>();
    projects.push(project("project-10", 100));

    store.save(&projects).unwrap();

    let expected = std::iter::once(project("project-10", 100))
        .chain(
            (2..22)
                .rev()
                .filter(|index| *index != 10)
                .map(|index| project(&format!("project-{index}"), index)),
        )
        .collect::<Vec<_>>();
    assert_eq!(store.load().unwrap(), expected);
}

#[test]
fn invalid_json_is_quarantined_without_exposing_contents() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    let private_contents = "private project contents";
    fs::write(&store.path, private_contents).unwrap();

    assert_eq!(store.load().unwrap(), Vec::<StoredProject>::new());
    assert!(!store.path.exists());
    let quarantined = fs::read_dir(temp_dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".corrupt-"))
        })
        .collect::<Vec<_>>();
    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        fs::read_to_string(&quarantined[0]).unwrap(),
        private_contents
    );
}

#[test]
fn unknown_version_is_rejected_and_never_overwritten() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    let future_contents = serde_json::to_vec_pretty(&json!({
        "version": 2,
        "entries": [{"futureSchema": true}],
    }))
    .unwrap();
    fs::write(&store.path, &future_contents).unwrap();

    assert_eq!(store.load(), Err(StorageError::UnsupportedVersion));
    assert_eq!(
        store.save(&[project("project-1", 1)]),
        Err(StorageError::UnsupportedVersion)
    );
    assert_eq!(fs::read(&store.path).unwrap(), future_contents);
}

#[test]
fn oversized_file_is_rejected_without_being_moved_or_overwritten() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    let oversized_contents = format!(
        r#"{{"version":2,"futureData":"{}"}}"#,
        "x".repeat(1024 * 1024)
    )
    .into_bytes();
    fs::write(&store.path, &oversized_contents).unwrap();

    assert_eq!(store.load(), Err(StorageError::Read));
    assert_eq!(
        store.save(&[project("project-1", 1)]),
        Err(StorageError::Read)
    );
    assert_eq!(fs::read(&store.path).unwrap(), oversized_contents);
}

#[test]
fn second_save_replaces_the_existing_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);

    store.save(&[project("project-1", 1)]).unwrap();
    store.save(&[project("project-2", 2)]).unwrap();

    assert_eq!(store.load().unwrap(), vec![project("project-2", 2)]);
    assert_eq!(temporary_files(&temp_dir), Vec::<String>::new());
}

#[test]
fn replacement_failure_preserves_old_file_and_cleans_up_temporary_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let working_store = store(&temp_dir);
    working_store.save(&[project("project-1", 1)]).unwrap();
    let old_contents = fs::read(&working_store.path).unwrap();
    let failing_store =
        RecentProjectStore::with_replacer(working_store.path.clone(), Arc::new(FailingReplacer));

    let error = failing_store.save(&[project("project-2", 2)]).unwrap_err();

    assert_eq!(error, StorageError::Write);
    assert_eq!(fs::read(&working_store.path).unwrap(), old_contents);
    assert_eq!(temporary_files(&temp_dir), Vec::<String>::new());
}

#[test]
fn platform_replacement_failure_preserves_the_existing_destination() {
    let temp_dir = tempfile::tempdir().unwrap();
    let source = temp_dir.path().join("missing-source.tmp");
    let destination = temp_dir.path().join("recent-projects-v1.json");
    let old_contents = b"old contents";
    fs::write(&destination, old_contents).unwrap();

    assert!(PlatformFileReplacer.replace(&source, &destination).is_err());
    assert_eq!(fs::read(destination).unwrap(), old_contents);
}

#[test]
fn save_failure_returns_a_sanitized_error() {
    let temp_dir = tempfile::tempdir().unwrap();
    let parent_file = temp_dir.path().join("not-a-directory");
    fs::write(&parent_file, "content").unwrap();
    let store = RecentProjectStore::new(parent_file.join("recent-projects-v1.json"));

    let error = store.save(&[project("project-1", 1)]).unwrap_err();

    assert_eq!(error, StorageError::Write);
    assert_eq!(error.to_string(), "recent projects could not be saved");
    assert!(!error.to_string().contains("not-a-directory"));
}

fn store(temp_dir: &TempDir) -> RecentProjectStore {
    RecentProjectStore::new(temp_dir.path().join("recent-projects-v1.json"))
}

fn project(id: &str, last_opened_at: i64) -> StoredProject {
    StoredProject {
        id: id.to_string(),
        path: format!(r"C:\work\{id}"),
        name: id.to_string(),
        last_opened_at,
    }
}

fn temporary_files(temp_dir: &TempDir) -> Vec<String> {
    fs::read_dir(temp_dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp-"))
        .collect()
}

#[derive(Debug)]
struct FailingReplacer;

impl FileReplacer for FailingReplacer {
    fn replace(&self, _source: &Path, _destination: &Path) -> io::Result<()> {
        Err(io::Error::other("injected replacement failure"))
    }
}
