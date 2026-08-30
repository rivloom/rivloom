use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tempfile::TempDir;

use super::FileReplacer;
use super::MAX_STORAGE_BYTES;
use super::MAX_STORED_TASKS;
use super::StorageError;
use super::StoredTask;
use super::TaskStore;
use crate::task::types::*;

#[test]
fn save_load_round_trip_is_versioned_bounded_and_contains_no_runtime_secrets() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    let tasks = (0..MAX_STORED_TASKS + 2)
        .map(stored_task)
        .collect::<Vec<_>>();

    store.save(&tasks).unwrap();

    let loaded = store.load().unwrap();
    assert_eq!(loaded, tasks[..MAX_STORED_TASKS]);
    let contents = fs::read_to_string(&store.path).unwrap();
    assert!(contents.contains(r#""version": 1"#));
    for forbidden in ["runtimeToken", "CODEX_HOME", "codexHome", "projectPath"] {
        assert!(!contents.contains(forbidden));
    }
}

#[test]
fn oversized_event_history_is_rejected_without_writing() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    let mut task = stored_task(1);
    task.record.events = vec![event(); MAX_EVENTS + 1];

    assert_eq!(store.save(&[task]), Err(StorageError::InvalidData));
    assert!(!store.path.exists());
}

#[test]
fn oversized_storage_document_is_rejected_without_writing() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = store(&temp_dir);
    let mut tasks = (0..MAX_STORED_TASKS).map(stored_task).collect::<Vec<_>>();
    for task in &mut tasks {
        task.record.summary = Some("s".repeat(MAX_SUMMARY_BYTES));
        task.record.runs = (0..6)
            .map(|index| RunRecord {
                id: format!("run-{index}"),
                status: RunStatus::Completed,
                summary: Some("s".repeat(MAX_SUMMARY_BYTES)),
                error: None,
            })
            .collect();
    }

    assert!(serde_json::to_vec(&tasks).unwrap().len() as u64 > MAX_STORAGE_BYTES);
    assert_eq!(store.save(&tasks), Err(StorageError::InvalidData));
    assert!(!store.path.exists());
}

#[test]
fn atomic_replacement_failure_preserves_the_previous_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let working_store = store(&temp_dir);
    working_store.save(&[stored_task(1)]).unwrap();
    let old_contents = fs::read(&working_store.path).unwrap();
    let failing_store =
        TaskStore::with_replacer(working_store.path.clone(), Arc::new(FailingReplacer));

    assert_eq!(
        failing_store.save(&[stored_task(2)]),
        Err(StorageError::Write)
    );
    assert_eq!(fs::read(&working_store.path).unwrap(), old_contents);
}

fn store(temp_dir: &TempDir) -> TaskStore {
    TaskStore::new(temp_dir.path().join("tasks/tasks-v1.json"))
}

fn stored_task(index: usize) -> StoredTask {
    StoredTask {
        idempotency_key: format!("task-key-{index}"),
        record: TaskRecord {
            id: format!("task-{index}"),
            spec: TaskSpec::new(format!("goal-{index}"), vec!["bounded".to_string()]),
            status: TaskStatus::Draft,
            summary: None,
            error: None,
            runs: vec![],
            events: vec![],
        },
        run_keys: vec![],
    }
}

fn event() -> TaskEvent {
    TaskEvent {
        sequence: 1,
        kind: TaskEventKind::TaskStatusChanged {
            from: TaskStatus::Draft,
            to: TaskStatus::Offered,
        },
    }
}

#[derive(Debug)]
struct FailingReplacer;

impl FileReplacer for FailingReplacer {
    fn replace(&self, _source: &Path, _destination: &Path) -> io::Result<()> {
        Err(io::Error::other("injected replacement failure"))
    }
}
