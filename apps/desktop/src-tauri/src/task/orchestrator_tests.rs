use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::*;
use crate::app_server::ConnectionIdentity;
use crate::app_server::NotificationObserver;
use crate::project::ProjectState;
use crate::task::artifact::PatchArtifactState;
use crate::task::receipt::RunReceiptOutcome;
use crate::task::storage::StoredTask;
use crate::task::storage::TaskStore;
use crate::task::types::TaskRecord;
use crate::task::types::TaskStatus;
use crate::task::types::TransitionDetails;

#[test]
fn one_local_codex_run_uses_an_isolated_thread_and_persists_a_verifiable_receipt() {
    let fixture = Fixture::new("Implement the bounded task", Ok(turn_response()));
    let request_count = fixture.connection.requests().len();

    let LocalCodexRunStart::Active(mut active) = fixture.runner.start(fixture.request()).unwrap()
    else {
        panic!("new run must become active");
    };

    assert_eq!(pending_status(active.poll().unwrap()), RunStatus::Running);
    fixture.connection.notify(
        "item/commandExecution/requestApproval",
        json!({"threadId": "thread-1", "turnId": "turn-1"}),
    );
    assert_eq!(
        pending_status(active.poll().unwrap()),
        RunStatus::WaitingApproval
    );
    fixture.connection.notify(
        "turn/completed",
        json!({
            "threadId": "thread-1",
            "turn": {"id": "turn-1", "status": "completed"}
        }),
    );
    let completion = finished(active.poll().unwrap());

    assert_eq!(completion.run.status, RunStatus::Completed);
    let receipt = completion.run.receipt.as_ref().unwrap();
    assert_eq!(receipt.outcome, RunReceiptOutcome::Success);
    assert_eq!(receipt.node_id, "node-1");
    assert_eq!(receipt.runtime_id, "codex");
    assert_eq!(receipt.runtime_version, "app-server/1.2.3");
    assert_eq!(completion.patch.state, PatchArtifactState::Complete);
    assert!(
        completion
            .patch
            .patch
            .as_deref()
            .unwrap()
            .contains("generated.txt")
    );
    assert_eq!(completion.cleanup, WorktreeCleanup::Removed);
    assert!(!fixture.repository.join("generated.txt").exists());
    let requests = fixture.connection.requests();
    let thread_cwd = requests[0].1["cwd"].as_str().unwrap();
    assert_eq!(
        requests[0],
        (
            "thread/start".to_string(),
            json!({
                "cwd": thread_cwd,
                "config": {"skills.include_instructions": false},
            }),
        )
    );
    assert_eq!(
        requests
            .iter()
            .map(|(method, _)| method.as_str())
            .collect::<Vec<_>>(),
        vec!["thread/start", "turn/start"]
    );
    let run_cwd = requests[1].1["cwd"].as_str().unwrap();
    assert_eq!(thread_cwd, run_cwd);
    assert_ne!(run_cwd, fixture.project.cwd());
    assert!(!PathBuf::from(run_cwd).exists());
    let stored = fs::read_to_string(&fixture.task_file).unwrap();
    assert!(!stored.contains(run_cwd));
    assert!(!stored.contains("generated-by-codex"));

    let LocalCodexRunStart::Existing(existing) = fixture.runner.start(fixture.request()).unwrap()
    else {
        panic!("completed run must be returned without restarting Codex");
    };
    assert_eq!(*existing, completion.run);
    assert_eq!(fixture.connection.requests().len(), request_count + 2);
}

#[test]
fn runtime_failure_never_becomes_success() {
    let fixture = Fixture::new("goal", Ok(turn_response()));
    let LocalCodexRunStart::Active(mut active) = fixture.runner.start(fixture.request()).unwrap()
    else {
        panic!("run must become active");
    };
    fixture.connection.terminal("failed");
    let completion = poll_until_finished(&mut active);

    assert_eq!(completion.run.status, RunStatus::Failed);
    assert_eq!(
        completion.run.receipt.unwrap().outcome,
        RunReceiptOutcome::Failed
    );
}

#[test]
fn user_stop_interrupts_only_the_matching_active_run_and_records_cancellation() {
    let fixture = Fixture::new("goal", Ok(turn_response()));
    let LocalCodexRunStart::Active(mut active) = fixture.runner.start(fixture.request()).unwrap()
    else {
        panic!("run must become active");
    };

    assert_eq!(active.task_id(), "task-1");
    assert_eq!(active.run_id(), "run-1");
    active.interrupt().unwrap();
    let completion = poll_until_finished(&mut active);

    assert_eq!(completion.run.status, RunStatus::Cancelled);
    assert_eq!(
        completion.run.receipt.unwrap().outcome,
        RunReceiptOutcome::Cancelled
    );
    assert_eq!(
        fixture
            .connection
            .requests()
            .into_iter()
            .map(|(method, _)| method)
            .collect::<Vec<_>>(),
        vec!["thread/start", "turn/start", "turn/interrupt"]
    );
}

#[test]
fn invalid_prompt_and_unknown_turn_start_are_bounded_before_or_after_the_runtime_boundary() {
    let oversized = Fixture::new(&"x".repeat(1_000), Ok(turn_response()));
    assert_eq!(
        oversized.runner.start(oversized.request()).err(),
        Some(TaskRunError::InvalidRequest)
    );
    assert_eq!(oversized.connection.requests(), vec![]);
    assert_eq!(
        oversized.tasks.get_task("task-1").unwrap().status,
        TaskStatus::Accepted
    );

    let mismatched = Fixture::new("goal", Ok(turn_response()));
    let mut mismatched_request = mismatched.request();
    let other_project = format!("project-v1-{}", "f".repeat(64));
    mismatched_request.project_id = &other_project;
    assert_eq!(
        mismatched.runner.start(mismatched_request).err(),
        Some(TaskRunError::InvalidRequest)
    );
    assert_eq!(mismatched.connection.requests(), vec![]);

    let disconnected = Fixture::new("goal", Err(ConnectionError::Disconnected));
    let LocalCodexRunStart::Finished(completion) =
        disconnected.runner.start(disconnected.request()).unwrap()
    else {
        panic!("an uncertain turn/start must finalize explicitly");
    };
    assert_eq!(completion.run.status, RunStatus::OutcomeUnknown);
    assert_eq!(disconnected.connection.requests().len(), 2);
}

struct Fixture {
    _temp_dir: tempfile::TempDir,
    repository: PathBuf,
    project: ResolvedProject,
    tasks: Arc<TaskService>,
    connection: Arc<RecordingConnection>,
    runner: LocalCodexRunService,
    task_file: PathBuf,
}

impl Fixture {
    fn new(goal: &str, turn_result: Result<Value, ConnectionError>) -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        let repository = temp_dir.path().join("repository");
        fs::create_dir(&repository).unwrap();
        git(&repository, &["init"]);
        git(
            &repository,
            &["config", "user.email", "tests@rivloom.local"],
        );
        git(&repository, &["config", "user.name", "Rivloom Tests"]);
        git(&repository, &["config", "core.autocrlf", "false"]);
        fs::write(repository.join("tracked.txt"), "baseline\n").unwrap();
        git(&repository, &["add", "tracked.txt"]);
        git(&repository, &["commit", "-m", "baseline"]);
        let projects = ProjectState::new(temp_dir.path().join("recent-projects-v1.json"));
        let selection = projects
            .select_project(Some(repository.clone()))
            .unwrap()
            .unwrap();
        let project = projects.lookup_project(&selection.project.id).unwrap();
        let mut record = TaskRecord::new("task-1", TaskSpec::new(goal, vec![])).unwrap();
        record
            .transition(TaskStatus::Offered, TransitionDetails::default())
            .unwrap();
        record
            .transition(TaskStatus::Accepted, TransitionDetails::default())
            .unwrap();
        record.register_run("run-1").unwrap();
        let task_file = temp_dir.path().join("tasks-v1.json");
        let store = TaskStore::new(task_file.clone());
        store
            .save(&[StoredTask {
                idempotency_key: "create-1".to_string(),
                project_id: Some(project.id().to_string()),
                record,
                run_keys: vec![],
            }])
            .unwrap();
        let tasks = Arc::new(TaskService::new(store));
        let events = Arc::new(EventRouter::default());
        let connection = Arc::new(RecordingConnection::new(events.clone(), turn_result));
        let runner = LocalCodexRunService::with_clock(
            tasks.clone(),
            TaskWorktreeManager::new(temp_dir.path().join("managed-worktrees")),
            events,
            Arc::new(SequenceClock::new([10, 20, 30, 40])),
        );
        Self {
            _temp_dir: temp_dir,
            repository,
            project,
            tasks,
            connection,
            runner,
            task_file,
        }
    }

    fn request(&self) -> StartLocalCodexRunRequest<'_> {
        StartLocalCodexRunRequest {
            project_id: self.project.id(),
            task_id: "task-1",
            run_id: "run-1",
            node_id: "node-1",
            runtime_version: "app-server/1.2.3",
            project: &self.project,
            connection: self.connection.clone(),
        }
    }
}

fn pending_status(progress: LocalCodexRunProgress) -> RunStatus {
    let LocalCodexRunProgress::Pending(run) = progress else {
        panic!("run must still be pending");
    };
    run.status
}

fn finished(progress: LocalCodexRunProgress) -> LocalCodexRunCompletion {
    let LocalCodexRunProgress::Finished(completion) = progress else {
        panic!("run must be finished");
    };
    completion
}

fn poll_until_finished(active: &mut ActiveLocalCodexRun) -> LocalCodexRunCompletion {
    loop {
        if let LocalCodexRunProgress::Finished(completion) = active.poll().unwrap() {
            return completion;
        }
    }
}

fn turn_response() -> Value {
    json!({"turn": {"id": "turn-1", "status": "inProgress"}})
}

fn git(cwd: &std::path::Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}

struct RecordingConnection {
    identity: ConnectionIdentity,
    events: Arc<EventRouter>,
    requests: Mutex<Vec<(String, Value)>>,
    turn_result: Mutex<Option<Result<Value, ConnectionError>>>,
}

impl RecordingConnection {
    fn new(events: Arc<EventRouter>, turn_result: Result<Value, ConnectionError>) -> Self {
        Self {
            identity: ConnectionIdentity::new(),
            events,
            requests: Mutex::default(),
            turn_result: Mutex::new(Some(turn_result)),
        }
    }

    fn requests(&self) -> Vec<(String, Value)> {
        self.requests.lock().unwrap().clone()
    }

    fn terminal(&self, status: &str) {
        self.notify(
            "turn/completed",
            json!({
                "threadId": "thread-1",
                "turn": {"id": "turn-1", "status": status}
            }),
        );
    }

    fn notify(&self, method: &str, params: Value) {
        NotificationObserver::on_notification(
            self.events.as_ref(),
            &self.identity,
            method,
            &params,
        );
    }
}

impl ConnectionControl for RecordingConnection {
    fn connection_identity(&self) -> ConnectionIdentity {
        self.identity.clone()
    }

    fn request(&self, method: &str, params: Value) -> Result<Value, ConnectionError> {
        self.requests
            .lock()
            .unwrap()
            .push((method.to_string(), params.clone()));
        match method {
            "thread/start" => {
                let cwd = params["cwd"].as_str().unwrap();
                Ok(json!({"thread": {"id": "thread-1", "cwd": cwd}, "cwd": cwd}))
            }
            "turn/start" => {
                let result = self
                    .turn_result
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap_or(Err(ConnectionError::Disconnected));
                if result.is_ok() {
                    fs::write(
                        PathBuf::from(params["cwd"].as_str().unwrap()).join("generated.txt"),
                        "generated-by-codex\n",
                    )
                    .unwrap();
                    self.notify(
                        "turn/started",
                        json!({
                            "threadId": "thread-1",
                            "turn": {"id": "turn-1", "status": "inProgress"}
                        }),
                    );
                }
                result
            }
            "turn/interrupt" => {
                self.terminal("interrupted");
                Ok(json!({}))
            }
            _ => Err(ConnectionError::Remote { code: -32601 }),
        }
    }

    fn request_without_params(&self, _method: &str) -> Result<Value, ConnectionError> {
        Err(ConnectionError::Serialize)
    }
}

struct SequenceClock {
    values: Mutex<VecDeque<i64>>,
}

impl SequenceClock {
    fn new(values: impl IntoIterator<Item = i64>) -> Self {
        Self {
            values: Mutex::new(values.into_iter().collect()),
        }
    }
}

impl TaskRunClock for SequenceClock {
    fn now_unix_seconds(&self) -> i64 {
        self.values.lock().unwrap().pop_front().unwrap()
    }
}
