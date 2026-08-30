use pretty_assertions::assert_eq;

use super::CreateTaskRequest;
use super::RegisterRunRequest;
use super::TaskService;
use super::TaskServiceError;
use crate::task::storage::StoredTask;
use crate::task::storage::TaskStore;
use crate::task::types::*;

#[test]
fn repeated_task_idempotency_key_returns_the_original_across_restarts() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("tasks-v1.json");
    let service = TaskService::new(TaskStore::new(path.clone()));
    let original = service
        .create_task(create_request("task-1", "create-1"))
        .unwrap();

    assert_eq!(
        service
            .create_task(create_request("task-2", "create-1"))
            .unwrap(),
        original
    );
    assert_eq!(service.get_task("task-1").unwrap(), original);

    let restarted = TaskService::new(TaskStore::new(path));
    assert_eq!(
        restarted
            .create_task(create_request("task-3", "create-1"))
            .unwrap(),
        original
    );
}

#[test]
fn repeated_run_idempotency_key_returns_the_original_across_restarts() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("tasks-v1.json");
    let store = TaskStore::new(path.clone());
    let task = accepted_task();
    store
        .save(&[StoredTask {
            idempotency_key: "create-1".to_string(),
            record: task,
            run_keys: vec![],
        }])
        .unwrap();
    let service = TaskService::new(store);
    let original = service
        .register_run(run_request("run-1", "run-key-1"))
        .unwrap();

    assert_eq!(
        service
            .register_run(run_request("run-2", "run-key-1"))
            .unwrap(),
        original
    );
    let restarted = TaskService::new(TaskStore::new(path));
    assert_eq!(
        restarted
            .register_run(run_request("run-3", "run-key-1"))
            .unwrap(),
        original
    );
}

#[test]
fn unknown_tasks_are_rejected_without_creating_a_run() {
    let temp_dir = tempfile::tempdir().unwrap();
    let service = TaskService::new(TaskStore::new(temp_dir.path().join("tasks-v1.json")));

    assert_eq!(
        service.register_run(run_request("run-1", "run-key-1")),
        Err(TaskServiceError::TaskNotFound)
    );
}

fn create_request(task_id: &str, idempotency_key: &str) -> CreateTaskRequest {
    CreateTaskRequest {
        task_id: task_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
        spec: TaskSpec::new("goal", vec!["bounded".to_string()]),
    }
}

fn run_request(run_id: &str, idempotency_key: &str) -> RegisterRunRequest {
    RegisterRunRequest {
        task_id: "task-1".to_string(),
        run_id: run_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
    }
}

fn accepted_task() -> TaskRecord {
    TaskRecord {
        id: "task-1".to_string(),
        spec: TaskSpec::new("goal", vec!["bounded".to_string()]),
        status: TaskStatus::Accepted,
        summary: None,
        error: None,
        runs: vec![],
        events: vec![],
    }
}
