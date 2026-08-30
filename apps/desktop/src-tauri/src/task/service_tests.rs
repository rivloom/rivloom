use pretty_assertions::assert_eq;
use sha2::Digest;
use sha2::Sha256;

use super::BeginRunResult;
use super::CreateTaskRequest;
use super::RegisterRunRequest;
use super::TaskService;
use super::TaskServiceError;
use crate::task::artifact::MAX_PATCH_BYTES;
use crate::task::artifact::PatchArtifact;
use crate::task::artifact::PatchArtifactState;
use crate::task::receipt::RunReceipt;
use crate::task::receipt::RunReceiptInput;
use crate::task::receipt::RunReceiptOutcome;
use crate::task::receipt::TestReport;
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
            project_id: Some(project_id()),
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
fn local_task_listing_and_acceptance_are_persisted_and_idempotent() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("tasks-v1.json");
    let service = TaskService::new(TaskStore::new(path.clone()));
    let draft = service
        .create_task(create_request("task-1", "create-1"))
        .unwrap();
    let mut accepted = draft.clone();
    accepted
        .transition(TaskStatus::Offered, TransitionDetails::default())
        .unwrap();
    accepted
        .transition(TaskStatus::Accepted, TransitionDetails::default())
        .unwrap();

    assert_eq!(service.list_tasks(&project_id()).unwrap(), vec![draft]);
    assert_eq!(
        service.accept_task(&project_id(), "task-1").unwrap(),
        accepted
    );
    assert_eq!(
        service.accept_task(&project_id(), "task-1").unwrap(),
        accepted
    );

    let restarted = TaskService::new(TaskStore::new(path));
    assert_eq!(restarted.list_tasks(&project_id()).unwrap(), vec![accepted]);
}

#[test]
fn project_binding_filters_listing_and_rejects_cross_project_task_use() {
    let temp_dir = tempfile::tempdir().unwrap();
    let service = TaskService::new(TaskStore::new(temp_dir.path().join("tasks-v1.json")));
    let task = service
        .create_task(create_request("task-1", "create-1"))
        .unwrap();
    let other_project = format!("project-v1-{}", "b".repeat(64));

    assert_eq!(service.list_tasks(&project_id()).unwrap(), vec![task]);
    assert_eq!(service.list_tasks(&other_project).unwrap(), vec![]);
    assert_eq!(
        service.get_project_task(&other_project, "task-1"),
        Err(TaskServiceError::TaskNotFound)
    );
    assert_eq!(
        service.accept_task(&other_project, "task-1"),
        Err(TaskServiceError::TaskNotFound)
    );
    let mut request = run_request("run-1", "run-key-1");
    request.project_id = other_project.clone();
    assert_eq!(
        service.register_run(request),
        Err(TaskServiceError::TaskNotFound)
    );
    let mut conflicting = create_request("task-2", "create-1");
    conflicting.project_id = other_project;
    assert_eq!(
        service.create_task(conflicting),
        Err(TaskServiceError::IdempotencyConflict)
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

#[test]
fn a_run_start_is_claimed_once_and_progress_transitions_are_idempotent() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("tasks-v1.json");
    let mut queued = accepted_task();
    queued.register_run("run-1").unwrap();
    TaskStore::new(path.clone())
        .save(&[StoredTask {
            idempotency_key: "create-1".to_string(),
            project_id: Some(project_id()),
            record: queued.clone(),
            run_keys: vec![],
        }])
        .unwrap();
    let service = TaskService::new(TaskStore::new(path));
    let mut expected = queued;
    expected
        .transition(TaskStatus::Running, TransitionDetails::default())
        .unwrap();
    expected
        .transition_run("run-1", RunStatus::Running, TransitionDetails::default())
        .unwrap();

    assert_eq!(
        service.begin_run("task-1", "run-1").unwrap(),
        BeginRunResult::Started(expected.runs[0].clone())
    );
    assert_eq!(service.get_task("task-1").unwrap(), expected);
    assert_eq!(
        service.begin_run("task-1", "run-1").unwrap(),
        BeginRunResult::Existing(expected.runs[0].clone())
    );
    let waiting = service
        .transition_run("task-1", "run-1", RunStatus::WaitingApproval)
        .unwrap();
    assert_eq!(waiting.status, RunStatus::WaitingApproval);
    assert_eq!(
        service
            .transition_run("task-1", "run-1", RunStatus::WaitingApproval)
            .unwrap(),
        waiting
    );
}

#[test]
fn every_terminal_receipt_is_persisted_atomically_and_idempotently_across_restarts() {
    let cases = [
        (
            RunReceiptOutcome::Success,
            RunStatus::Completed,
            TaskStatus::AwaitingReview,
        ),
        (
            RunReceiptOutcome::Failed,
            RunStatus::Failed,
            TaskStatus::Failed,
        ),
        (
            RunReceiptOutcome::Cancelled,
            RunStatus::Cancelled,
            TaskStatus::Cancelled,
        ),
        (
            RunReceiptOutcome::OutcomeUnknown,
            RunStatus::OutcomeUnknown,
            TaskStatus::OutcomeUnknown,
        ),
    ];

    for (outcome, run_status, task_status) in cases {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("tasks-v1.json");
        TaskStore::new(path.clone())
            .save(&[StoredTask {
                idempotency_key: "create-1".to_string(),
                project_id: Some(project_id()),
                record: running_task(),
                run_keys: vec![],
            }])
            .unwrap();
        let receipt = receipt(outcome, 90);
        let service = TaskService::new(TaskStore::new(path.clone()));

        let finalized = service.finalize_run(receipt.clone()).unwrap();

        assert_eq!(
            finalized,
            RunRecord {
                id: "run-1".to_string(),
                status: run_status,
                summary: receipt.summary.clone(),
                error: receipt.error.clone(),
                receipt: Some(receipt.clone()),
            }
        );
        let task = service.get_task("task-1").unwrap();
        assert_eq!(task.status, task_status);
        let restarted = TaskService::new(TaskStore::new(path.clone()));
        assert_eq!(restarted.finalize_run(receipt).unwrap(), finalized);
        let contents = std::fs::read_to_string(path).unwrap();
        assert!(!contents.contains("patch-body-must-not-enter-task-store"));
    }
}

#[test]
fn a_different_receipt_never_overwrites_the_first_terminal_result() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("tasks-v1.json");
    TaskStore::new(path.clone())
        .save(&[StoredTask {
            idempotency_key: "create-1".to_string(),
            project_id: Some(project_id()),
            record: running_task(),
            run_keys: vec![],
        }])
        .unwrap();
    let service = TaskService::new(TaskStore::new(path));
    let original = receipt(RunReceiptOutcome::Success, 90);
    service.finalize_run(original.clone()).unwrap();

    assert_eq!(
        service.finalize_run(receipt(RunReceiptOutcome::Success, 91)),
        Err(TaskServiceError::StateMachine)
    );
    assert_eq!(
        service.get_task("task-1").unwrap().runs[0].receipt,
        Some(original)
    );
}

fn create_request(task_id: &str, idempotency_key: &str) -> CreateTaskRequest {
    CreateTaskRequest {
        task_id: task_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
        project_id: project_id(),
        spec: TaskSpec::new("goal", vec!["bounded".to_string()]),
    }
}

fn run_request(run_id: &str, idempotency_key: &str) -> RegisterRunRequest {
    RegisterRunRequest {
        task_id: "task-1".to_string(),
        run_id: run_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
        project_id: project_id(),
    }
}

fn project_id() -> String {
    format!("project-v1-{}", "a".repeat(64))
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

fn running_task() -> TaskRecord {
    let mut task = accepted_task();
    task.status = TaskStatus::Running;
    task.runs.push(RunRecord {
        id: "run-1".to_string(),
        status: RunStatus::Running,
        summary: None,
        error: None,
        receipt: None,
    });
    task
}

fn receipt(outcome: RunReceiptOutcome, finished_at: i64) -> RunReceipt {
    let (summary, error) = match outcome {
        RunReceiptOutcome::Success => (Some("ready for review".to_string()), None),
        RunReceiptOutcome::Failed => (None, Some("runtime failed".to_string())),
        RunReceiptOutcome::Cancelled => (Some("stopped locally".to_string()), None),
        RunReceiptOutcome::OutcomeUnknown => (None, Some("runtime disconnected".to_string())),
    };
    let patch = "patch-body-must-not-enter-task-store".to_string();
    RunReceipt::new(RunReceiptInput {
        task_id: "task-1".to_string(),
        run_id: "run-1".to_string(),
        node_id: "node-1".to_string(),
        runtime_id: "codex".to_string(),
        runtime_version: "1.2.3".to_string(),
        started_at: 1,
        finished_at,
        outcome,
        summary,
        error,
        tests: TestReport::NotReported,
        patch: PatchArtifact {
            baseline_commit: "a".repeat(40),
            state: PatchArtifactState::Complete,
            limit_bytes: MAX_PATCH_BYTES,
            byte_count: Some(patch.len() as u64),
            sha256: Some(format!("{:x}", Sha256::digest(patch.as_bytes()))),
            patch: Some(patch),
        },
    })
    .unwrap()
}
