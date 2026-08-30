use std::collections::VecDeque;
use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::*;
use crate::app_server::NotificationObserver;
use crate::app_server::event_router::RunEventKind;
use crate::project::ProjectState;
use crate::task::worktree::TaskWorktree;
use crate::task::worktree::TaskWorktreeManager;

#[test]
fn start_uses_only_the_isolated_worktree_and_keeps_an_early_started_event() {
    let (_temp_dir, worktree) = isolated_worktree();
    let router = Arc::new(EventRouter::default());
    let connection = Arc::new(RecordingConnection::new(
        vec![Ok(start_response())],
        Some(router.clone()),
    ));
    let runtime = CodexRuntime::default();

    let run = runtime
        .start_run(
            CodexRunRequest {
                run_id: "run-1",
                thread_id: "thread-1",
                prompt: "Implement the bounded task",
                worktree: &worktree,
            },
            connection.clone(),
            &router,
        )
        .unwrap();

    assert_eq!(run.run_id(), "run-1");
    assert_eq!(run.thread_id(), "thread-1");
    assert_eq!(run.turn_id(), "turn-1");
    assert_eq!(
        run.try_next_event(),
        Some(RunEvent {
            run_id: "run-1".to_string(),
            sequence: 1,
            kind: RunEventKind::Running,
        })
    );
    assert_eq!(
        connection.requests(),
        vec![request(
            "turn/start",
            json!({
                "threadId": "thread-1",
                "clientUserMessageId": "run-1",
                "input": [{"type": "text", "text": "Implement the bounded task"}],
                "cwd": worktree.cwd(),
                "approvalPolicy": "on-request",
                "approvalsReviewer": "auto_review",
                "sandboxPolicy": {
                    "type": "workspaceWrite",
                    "writableRoots": [worktree.cwd()],
                    "networkAccess": false,
                },
            }),
        )]
    );
    assert_eq!(
        runtime
            .start_run(
                CodexRunRequest {
                    run_id: "run-1",
                    thread_id: "thread-1",
                    prompt: "must not run twice",
                    worktree: &worktree,
                },
                connection.clone(),
                &router,
            )
            .err(),
        Some(CodexRuntimeError::EventRouting)
    );
    assert_eq!(connection.requests().len(), 1);
}

#[test]
fn invalid_or_oversized_requests_never_reach_app_server() {
    let (_temp_dir, worktree) = isolated_worktree();
    let router = EventRouter::default();
    let connection = Arc::new(RecordingConnection::new(vec![], None));
    let runtime = CodexRuntime::default();

    for (run_id, thread_id, prompt) in [
        ("", "thread-1", "goal".to_string()),
        ("run-1", "", "goal".to_string()),
        ("run-1", "thread-1", " ".to_string()),
        ("run-1", "thread-1", "x".repeat(MAX_RUN_PROMPT_BYTES + 1)),
    ] {
        assert_eq!(
            runtime
                .start_run(
                    CodexRunRequest {
                        run_id,
                        thread_id,
                        prompt: &prompt,
                        worktree: &worktree,
                    },
                    connection.clone(),
                    &router,
                )
                .err(),
            Some(CodexRuntimeError::InvalidRequest)
        );
    }
    assert_eq!(connection.requests(), vec![]);
}

#[test]
fn interrupt_requires_the_exact_active_runtime_and_connection() {
    let (_temp_dir, worktree) = isolated_worktree();
    let router = Arc::new(EventRouter::default());
    let connection = Arc::new(RecordingConnection::new(
        vec![Ok(start_response()), Ok(json!({}))],
        Some(router.clone()),
    ));
    let runtime = CodexRuntime::default();
    let run = runtime
        .start_run(
            CodexRunRequest {
                run_id: "run-1",
                thread_id: "thread-1",
                prompt: "goal",
                worktree: &worktree,
            },
            connection.clone(),
            &router,
        )
        .unwrap();

    assert_eq!(
        CodexRuntime::default().interrupt_run(&run, connection.clone()),
        Err(CodexRuntimeError::RunNotActive)
    );
    assert_eq!(
        runtime.interrupt_run(&run, Arc::new(RecordingConnection::new(vec![], None)),),
        Err(CodexRuntimeError::RunNotActive)
    );
    assert_eq!(runtime.interrupt_run(&run, connection.clone()), Ok(()));
    assert_eq!(
        connection.requests().last(),
        Some(&request(
            "turn/interrupt",
            json!({"threadId": "thread-1", "turnId": "turn-1"}),
        ))
    );

    NotificationObserver::on_notification(
        router.as_ref(),
        &connection.connection_identity(),
        "turn/completed",
        &json!({
            "threadId": "thread-1",
            "turn": {"id": "turn-1", "status": "interrupted"},
        }),
    );
    assert!(!run.is_active());
    assert_eq!(
        runtime.interrupt_run(&run, connection.clone()),
        Err(CodexRuntimeError::RunNotActive)
    );
    assert_eq!(connection.requests().len(), 2);
}

#[test]
fn malformed_and_disconnected_responses_are_sanitized() {
    let (_temp_dir, worktree) = isolated_worktree();
    let router = EventRouter::default();
    let runtime = CodexRuntime::default();

    for (response, expected) in [
        (
            Ok(json!({"turn": {"id": "turn-1", "status": "failed", "secret": "SECRET"}})),
            CodexRuntimeError::OutcomeUnknown,
        ),
        (
            Err(ConnectionError::Disconnected),
            CodexRuntimeError::OutcomeUnknown,
        ),
        (
            Err(ConnectionError::Remote { code: 500 }),
            CodexRuntimeError::RequestFailed,
        ),
    ] {
        let connection = Arc::new(RecordingConnection::new(vec![response], None));
        let result = runtime.start_run(
            CodexRunRequest {
                run_id: "run-1",
                thread_id: "thread-1",
                prompt: "goal",
                worktree: &worktree,
            },
            connection,
            &router,
        );
        let error = result.err().expect("invalid response must fail");
        assert_eq!(error, expected);
        assert!(!error.to_string().contains("SECRET"));
    }
}

#[test]
fn an_early_turn_that_disagrees_with_the_response_is_never_rebound() {
    let (_temp_dir, worktree) = isolated_worktree();
    let router = Arc::new(EventRouter::default());
    let connection = Arc::new(RecordingConnection::new(
        vec![Ok(json!({
            "turn": {"id": "turn-other", "status": "inProgress"}
        }))],
        Some(router.clone()),
    ));

    let error = CodexRuntime::default()
        .start_run(
            CodexRunRequest {
                run_id: "run-1",
                thread_id: "thread-1",
                prompt: "goal",
                worktree: &worktree,
            },
            connection,
            &router,
        )
        .err();

    assert_eq!(error, Some(CodexRuntimeError::OutcomeUnknown));
}

fn isolated_worktree() -> (tempfile::TempDir, TaskWorktree) {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_path = temp_dir.path().join("repository");
    fs::create_dir(&project_path).unwrap();
    git(&project_path, &["init"]);
    git(
        &project_path,
        &["config", "user.email", "tests@rivloom.local"],
    );
    git(&project_path, &["config", "user.name", "Rivloom Tests"]);
    git(&project_path, &["config", "core.autocrlf", "false"]);
    fs::write(project_path.join("tracked.txt"), "baseline\n").unwrap();
    git(&project_path, &["add", "tracked.txt"]);
    git(&project_path, &["commit", "-m", "baseline"]);
    let state = ProjectState::new(temp_dir.path().join("recent-projects-v1.json"));
    let selection = state.select_project(Some(project_path)).unwrap().unwrap();
    let project = state.lookup_project(&selection.project.id).unwrap();
    let worktree = TaskWorktreeManager::new(temp_dir.path().join("managed-worktrees"))
        .create(&project, "fixture-run")
        .unwrap();
    (temp_dir, worktree)
}

fn git(cwd: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success());
}

fn start_response() -> Value {
    json!({"turn": {"id": "turn-1", "status": "inProgress", "items": []}})
}

fn request(method: &str, params: Value) -> (String, Value) {
    (method.to_string(), params)
}

struct RecordingConnection {
    identity: ConnectionIdentity,
    requests: Mutex<Vec<(String, Value)>>,
    responses: Mutex<VecDeque<Result<Value, ConnectionError>>>,
    early_router: Option<Arc<EventRouter>>,
}

impl RecordingConnection {
    fn new(
        responses: Vec<Result<Value, ConnectionError>>,
        early_router: Option<Arc<EventRouter>>,
    ) -> Self {
        Self {
            identity: ConnectionIdentity::new(),
            requests: Mutex::default(),
            responses: Mutex::new(responses.into()),
            early_router,
        }
    }

    fn requests(&self) -> Vec<(String, Value)> {
        self.requests.lock().unwrap().clone()
    }
}

impl ConnectionControl for RecordingConnection {
    fn connection_identity(&self) -> ConnectionIdentity {
        self.identity.clone()
    }

    fn request(&self, method: &str, params: Value) -> Result<Value, ConnectionError> {
        self.requests.lock().unwrap().push(request(method, params));
        if method == "turn/start"
            && let Some(router) = &self.early_router
        {
            NotificationObserver::on_notification(
                router.as_ref(),
                &self.identity,
                "turn/started",
                &json!({
                    "threadId": "thread-1",
                    "turn": {"id": "turn-1", "status": "inProgress"},
                }),
            );
        }
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(ConnectionError::Disconnected))
    }

    fn request_without_params(&self, _method: &str) -> Result<Value, ConnectionError> {
        Err(ConnectionError::Serialize)
    }
}
