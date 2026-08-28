use std::collections::VecDeque;
use std::fs;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tempfile::TempDir;

use super::{MAX_ACCUMULATED_THREADS, ThreadService, ThreadServiceError};
use crate::app_server::{ConnectionControl, ConnectionError, ConnectionIdentity};
use crate::project::service::{ProjectService, ResolvedProject};
use crate::project::storage::RecentProjectStore;

const OMITTED_PARAMS_SENTINEL: &str = "__omitted_params__";

#[test]
fn methods_send_only_the_three_exact_stable_requests() {
    let fixture = project();
    let cwd = fixture.resolved.cwd();
    let connection = FakeConnection::new([
        Ok(json!({ "data": [thread("thr-list", cwd)], "nextCursor": "next" })),
        Ok(json!({ "thread": thread("thr-start", cwd), "cwd": cwd })),
        Ok(json!({ "thread": thread("thr-read", cwd) })),
    ]);

    ThreadService::list_threads(&fixture.resolved, connection.clone(), None, 0).unwrap();
    ThreadService::start_thread(&fixture.resolved, connection.clone()).unwrap();
    ThreadService::read_thread(&fixture.resolved, connection.clone(), "thr-read").unwrap();

    assert_eq!(
        connection.requests(),
        vec![
            request(
                "thread/list",
                json!({
                    "cwd": cwd,
                    "limit": 50,
                    "sortKey": "recency_at",
                    "sortDirection": "desc"
                }),
            ),
            request("thread/start", json!({ "cwd": cwd })),
            request(
                "thread/read",
                json!({ "threadId": "thr-read", "includeTurns": false }),
            ),
        ]
    );
    assert!(connection.requests().iter().all(|request| {
        request.method != "thread/resume"
            && request.method != "turn/start"
            && !request.method.starts_with("project/")
    }));
}

#[test]
fn pagination_passes_through_opaque_cursors_and_stops_at_500() {
    let fixture = project();
    let cwd = fixture.resolved.cwd();
    let connection = FakeConnection::new([Ok(json!({
        "data": (0..49).map(|index| thread(&format!("thr-{index}"), cwd)).collect::<Vec<_>>(),
        "nextCursor": "server-has-more"
    }))]);

    let page = ThreadService::list_threads(
        &fixture.resolved,
        connection.clone(),
        Some("opaque-cursor"),
        451,
    )
    .unwrap();
    let capped = ThreadService::list_threads(
        &fixture.resolved,
        connection.clone(),
        Some("must-not-be-sent"),
        MAX_ACCUMULATED_THREADS,
    )
    .unwrap();

    assert_eq!(page.data.len(), 49);
    assert_eq!(page.next_cursor, None);
    assert_eq!(capped.data.len(), 0);
    assert_eq!(
        connection.requests(),
        vec![request(
            "thread/list",
            json!({
                "cwd": cwd,
                "cursor": "opaque-cursor",
                "limit": 49,
                "sortKey": "recency_at",
                "sortDirection": "desc"
            }),
        )]
    );
}

#[test]
fn invalid_inputs_are_rejected_before_a_request() {
    let fixture = project();
    let connection = FakeConnection::new([]);

    assert_eq!(
        ThreadService::list_threads(
            &fixture.resolved,
            connection.clone(),
            Some(&"x".repeat(4 * 1024 + 1)),
            0,
        ),
        Err(ThreadServiceError::InvalidRequest)
    );
    assert_eq!(
        ThreadService::list_threads(
            &fixture.resolved,
            connection.clone(),
            None,
            MAX_ACCUMULATED_THREADS + 1,
        ),
        Err(ThreadServiceError::InvalidRequest)
    );
    assert_eq!(
        ThreadService::read_thread(&fixture.resolved, connection.clone(), &"x".repeat(1025)),
        Err(ThreadServiceError::InvalidRequest)
    );
    assert_eq!(connection.requests(), Vec::<RecordedRequest>::new());
}

#[test]
fn connection_and_protocol_failures_are_sanitized() {
    let fixture = project();
    let connection = FakeConnection::new([
        Err(ConnectionError::Remote { code: -32_000 }),
        Err(ConnectionError::Disconnected),
        Ok(json!({ "thread": thread("thr-other", "C:/other") })),
    ]);

    assert_eq!(
        ThreadService::list_threads(&fixture.resolved, connection.clone(), None, 0),
        Err(ThreadServiceError::RequestFailed)
    );
    assert_eq!(
        ThreadService::start_thread(&fixture.resolved, connection.clone()),
        Err(ThreadServiceError::Disconnected)
    );
    assert_eq!(
        ThreadService::read_thread(&fixture.resolved, connection, "thr-other"),
        Err(ThreadServiceError::ProjectMismatch)
    );
}

fn thread(id: &str, cwd: &str) -> Value {
    json!({
        "id": id,
        "name": null,
        "preview": "Preview",
        "createdAt": 10,
        "updatedAt": 20,
        "recencyAt": 30,
        "status": { "type": "idle" },
        "cwd": cwd
    })
}

struct ProjectFixture {
    _temp_dir: TempDir,
    resolved: ResolvedProject,
}

fn project() -> ProjectFixture {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir.path().join("workspace");
    fs::create_dir(&project_dir).unwrap();
    let service = ProjectService::new(RecentProjectStore::new(
        temp_dir.path().join("settings/recent-projects-v1.json"),
    ));
    let selected = service.select_project(Some(project_dir)).unwrap().unwrap();
    let resolved = service.lookup_project(&selected.project.id).unwrap();
    ProjectFixture {
        _temp_dir: temp_dir,
        resolved,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedRequest {
    method: String,
    params: Value,
}

struct FakeConnection {
    identity: ConnectionIdentity,
    responses: Mutex<VecDeque<Result<Value, ConnectionError>>>,
    requests: Mutex<Vec<RecordedRequest>>,
}

impl FakeConnection {
    fn new(responses: impl IntoIterator<Item = Result<Value, ConnectionError>>) -> Arc<Self> {
        Arc::new(Self {
            identity: ConnectionIdentity::new(),
            responses: Mutex::new(responses.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        })
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl ConnectionControl for FakeConnection {
    fn connection_identity(&self) -> ConnectionIdentity {
        self.identity.clone()
    }

    fn request(&self, method: &str, params: Value) -> Result<Value, ConnectionError> {
        self.requests.lock().unwrap().push(request(method, params));
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("a fake response should be queued")
    }

    fn request_without_params(&self, method: &str) -> Result<Value, ConnectionError> {
        self.request(method, json!(OMITTED_PARAMS_SENTINEL))
    }
}

fn request(method: &str, params: Value) -> RecordedRequest {
    RecordedRequest {
        method: method.to_string(),
        params,
    }
}
