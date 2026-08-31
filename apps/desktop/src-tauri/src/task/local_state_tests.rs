use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::*;
use crate::app_server::ConnectionError;
use crate::app_server::ConnectionIdentity;
use crate::app_server::NotificationObserver;
use crate::project::ProjectState;
use crate::task::receipt::RunReceiptOutcome;
use crate::task::types::TaskStatus;

#[test]
fn local_task_start_publishes_a_volatile_patch_and_never_executes_twice() {
    let fixture = Fixture::new(
        Arc::new(|task| {
            task();
            Ok(())
        }),
        CompletionMode::AfterStart,
    );

    let started = fixture.state.start(fixture.request()).unwrap();

    assert_eq!(started.task.status, TaskStatus::AwaitingReview);
    assert_eq!(
        started.task.runs[0].receipt.as_ref().unwrap().outcome,
        RunReceiptOutcome::Success
    );
    assert_eq!(
        fixture.state.list(fixture.project.id()).unwrap(),
        vec![started.task.clone()]
    );
    let updates = fixture.updates.lock().unwrap();
    let completed = updates.last().unwrap();
    assert_eq!(completed.task, started.task);
    assert!(
        completed
            .patch
            .as_ref()
            .and_then(|patch| patch.patch.as_deref())
            .unwrap()
            .contains("generated.txt")
    );
    drop(updates);
    let stored = fs::read_to_string(&fixture.task_file).unwrap();
    assert!(!stored.contains(fixture.project.cwd()));
    assert!(!stored.contains("generated-by-codex"));

    let repeated = fixture.state.start(fixture.request()).unwrap();

    assert_eq!(repeated, started);
    assert_eq!(
        fixture.connection.methods(),
        vec!["thread/start", "turn/start"]
    );
}

#[test]
fn local_stop_is_correlated_and_finishes_through_the_background_worker() {
    let queued = Arc::new(Mutex::new(None));
    let queued_for_spawn = queued.clone();
    let spawn: Arc<SpawnTask> = Arc::new(move |task| {
        *queued_for_spawn.lock().unwrap() = Some(task);
        Ok(())
    });
    let fixture = Fixture::new(spawn, CompletionMode::OnlyWhenInterrupted);
    let started = fixture.state.start(fixture.request()).unwrap();
    assert_eq!(started.task.status, TaskStatus::Running);

    assert_eq!(
        fixture
            .state
            .stop(fixture.project.id(), &started.task.id, &started.run_id)
            .unwrap(),
        started.task
    );
    queued.lock().unwrap().take().unwrap()();

    let updates = fixture.updates.lock().unwrap();
    let cancelled = &updates.last().unwrap().task;
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    assert_eq!(
        cancelled.runs[0].receipt.as_ref().unwrap().outcome,
        RunReceiptOutcome::Cancelled
    );
    drop(updates);
    assert_eq!(
        fixture.connection.methods(),
        vec!["thread/start", "turn/start", "turn/interrupt"]
    );
    assert_eq!(
        fixture
            .state
            .stop(fixture.project.id(), &started.task.id, &started.run_id),
        Err(LocalTaskError::RunNotActive)
    );
}

#[test]
fn worker_spawn_failure_becomes_explicitly_unknown_without_losing_the_task() {
    let fixture = Fixture::new(
        Arc::new(|_task| Err(())),
        CompletionMode::OnlyWhenInterrupted,
    );

    let started = fixture.state.start(fixture.request()).unwrap();

    assert_eq!(started.task.status, TaskStatus::OutcomeUnknown);
    assert_eq!(
        started.task.runs[0].receipt.as_ref().unwrap().outcome,
        RunReceiptOutcome::OutcomeUnknown
    );
    assert_eq!(
        fixture.state.list(fixture.project.id()).unwrap(),
        vec![started.task]
    );
}

struct Fixture {
    _temp_dir: tempfile::TempDir,
    project: ResolvedProject,
    state: TaskRunState,
    connection: Arc<RecordingConnection>,
    updates: Arc<Mutex<Vec<LocalTaskUpdate>>>,
    constraints: Vec<String>,
    task_file: PathBuf,
}

impl Fixture {
    fn new(spawn: Arc<SpawnTask>, completion: CompletionMode) -> Self {
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
        let selection = projects.select_project(Some(repository)).unwrap().unwrap();
        let project = projects.lookup_project(&selection.project.id).unwrap();
        let events = Arc::new(EventRouter::default());
        let connection = Arc::new(RecordingConnection::new(events.clone(), completion));
        let updates = Arc::new(Mutex::new(Vec::new()));
        let updates_for_callback = updates.clone();
        let task_file = temp_dir.path().join("tasks-v1.json");
        let state = TaskRunState::with_spawner(
            task_file.clone(),
            temp_dir.path().join("managed-worktrees"),
            events,
            Arc::new(move |update| updates_for_callback.lock().unwrap().push(update.clone())),
            spawn,
        )
        .unwrap();
        Self {
            _temp_dir: temp_dir,
            project,
            state,
            connection,
            updates,
            constraints: vec!["stay bounded".to_string()],
            task_file,
        }
    }

    fn request(&self) -> StartLocalTaskRequest<'_> {
        StartLocalTaskRequest {
            project_id: self.project.id(),
            idempotency_key: "request-1",
            goal: "Create the requested file",
            constraints: &self.constraints,
            node_id: "device-v1-11111111111111111111111111111111",
            runtime_version: "app-server/1.2.3",
            project: &self.project,
            connection: self.connection.clone(),
        }
    }
}

#[derive(Clone, Copy)]
enum CompletionMode {
    AfterStart,
    OnlyWhenInterrupted,
}

struct RecordingConnection {
    identity: ConnectionIdentity,
    events: Arc<EventRouter>,
    requests: Mutex<Vec<(String, Value)>>,
    completion: CompletionMode,
    responses: Mutex<VecDeque<Value>>,
}

impl RecordingConnection {
    fn new(events: Arc<EventRouter>, completion: CompletionMode) -> Self {
        Self {
            identity: ConnectionIdentity::new(),
            events,
            requests: Mutex::default(),
            completion,
            responses: Mutex::new(VecDeque::from([json!({
                "turn": {"id": "turn-1", "status": "inProgress"}
            })])),
        }
    }

    fn methods(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|(method, _)| method.clone())
            .collect()
    }

    fn notify(&self, method: &str, params: Value) {
        self.events.on_notification(&self.identity, method, &params);
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
                if matches!(self.completion, CompletionMode::AfterStart) {
                    let events = self.events.clone();
                    let identity = self.identity.clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(/*millis*/ 5));
                        events.on_notification(
                            &identity,
                            "turn/completed",
                            &json!({
                                "threadId": "thread-1",
                                "turn": {"id": "turn-1", "status": "completed"}
                            }),
                        );
                    });
                }
                self.responses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .ok_or(ConnectionError::Disconnected)
            }
            "turn/interrupt" => {
                self.notify(
                    "turn/completed",
                    json!({
                        "threadId": "thread-1",
                        "turn": {"id": "turn-1", "status": "interrupted"}
                    }),
                );
                Ok(json!({}))
            }
            _ => Err(ConnectionError::Remote { code: -32601 }),
        }
    }

    fn request_without_params(&self, _method: &str) -> Result<Value, ConnectionError> {
        Err(ConnectionError::Serialize)
    }
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
