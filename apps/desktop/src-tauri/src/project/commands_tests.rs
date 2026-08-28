use std::collections::VecDeque;
use std::fs;
#[cfg(any(not(windows), feature = "test-tauri-commands"))]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serde_json::Value;
#[cfg(any(not(windows), feature = "test-tauri-commands"))]
use serde_json::json;
#[cfg(any(not(windows), feature = "test-tauri-commands"))]
use tauri::ipc::{CallbackFn, InvokeBody};
#[cfg(any(not(windows), feature = "test-tauri-commands"))]
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
#[cfg(any(not(windows), feature = "test-tauri-commands"))]
use tauri::webview::InvokeRequest;
#[cfg(any(not(windows), feature = "test-tauri-commands"))]
use tauri::{App, Manager, WebviewWindow, WebviewWindowBuilder};
use tempfile::TempDir;

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
use super::{ActiveConnectionProvider, FolderPicker, ProjectConnectionState, ProjectDialogState};
use super::{ProjectCommandError, execute_thread_command, list_recent, register_selection};
use crate::app_server::{ConnectionControl, ConnectionError, ConnectionIdentity};
use crate::project::state::ProjectState;
use crate::project::thread_service::ThreadService;
use crate::project::types::{LocalProject, ProjectThreadPage};

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
#[test]
fn tauri_commands_resolve_managed_states_and_forward_parameters() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir.path().join("workspace");
    fs::create_dir(&project_dir).unwrap();
    let cwd = dunce::canonicalize(&project_dir)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let connection = FakeConnection::new([
        Ok(json!({ "data": [app_server_thread("thr-list", &cwd)], "nextCursor": null })),
        Ok(json!({ "thread": app_server_thread("thr-start", &cwd), "cwd": cwd })),
        Ok(json!({ "thread": app_server_thread("thr-read", &cwd) })),
    ]);
    let app = command_app(
        &temp_dir,
        Ok(Some(project_dir)),
        Some(connection.clone() as Arc<dyn ConnectionControl>),
    );
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    let selection = invoke_project_command(&webview, "select_project", json!({})).unwrap();
    let project = selection["project"].clone();
    let project_id = project["id"].as_str().unwrap();
    assert_eq!(
        invoke_project_command(&webview, "list_recent_projects", json!({})),
        Ok(json!([project]))
    );
    assert_eq!(
        invoke_project_command(
            &webview,
            "list_project_threads",
            json!({ "projectId": project_id, "cursor": "cursor-1", "loadedCount": 7 }),
        ),
        Ok(json!({ "data": [command_thread("thr-list", &cwd)], "nextCursor": null }))
    );
    assert_eq!(
        invoke_project_command(
            &webview,
            "start_project_thread",
            json!({ "projectId": project_id }),
        ),
        Ok(command_thread("thr-start", &cwd))
    );
    assert_eq!(
        invoke_project_command(
            &webview,
            "read_project_thread",
            json!({ "projectId": project_id, "threadId": "thr-read" }),
        ),
        Ok(command_thread("thr-read", &cwd))
    );
    assert_eq!(
        invoke_project_command(
            &webview,
            "remove_recent_project",
            json!({ "projectId": project_id }),
        ),
        Ok(Value::Null)
    );
    assert_eq!(
        invoke_project_command(&webview, "list_recent_projects", json!({})),
        Ok(json!([]))
    );
    assert_eq!(connection.requests(), expected_thread_requests(&cwd));
}

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
#[test]
fn tauri_thread_wrappers_share_the_disconnected_error() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir.path().join("workspace");
    fs::create_dir(&project_dir).unwrap();
    let app = command_app(&temp_dir, Ok(Some(project_dir)), None);
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    let selection = invoke_project_command(&webview, "select_project", json!({})).unwrap();
    let project_id = selection["project"]["id"].as_str().unwrap();

    assert_eq!(
        invoke_project_command(
            &webview,
            "list_project_threads",
            json!({
                "projectId": project_id,
                "cursor": "cursor-1",
                "loadedCount": 7
            }),
        ),
        Err(json!("coreUnavailable"))
    );
    assert_eq!(
        invoke_project_command(
            &webview,
            "start_project_thread",
            json!({ "projectId": project_id }),
        ),
        Err(json!("coreUnavailable"))
    );
    assert_eq!(
        invoke_project_command(
            &webview,
            "read_project_thread",
            json!({ "projectId": project_id, "threadId": "thr-read" }),
        ),
        Err(json!("coreUnavailable"))
    );
}

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
#[test]
fn tauri_select_wrapper_preserves_cancel_and_sanitizes_picker_failure() {
    let canceled_dir = tempfile::tempdir().unwrap();
    let canceled_app = command_app(&canceled_dir, Ok(None), None);
    let canceled_webview = WebviewWindowBuilder::new(&canceled_app, "main", Default::default())
        .build()
        .unwrap();
    assert_eq!(
        invoke_project_command(&canceled_webview, "select_project", json!({})),
        Ok(Value::Null)
    );

    let failed_dir = tempfile::tempdir().unwrap();
    let failed_app = command_app(&failed_dir, Err(ProjectCommandError::Project), None);
    let failed_webview = WebviewWindowBuilder::new(&failed_app, "main", Default::default())
        .build()
        .unwrap();
    assert_eq!(
        invoke_project_command(&failed_webview, "select_project", json!({})),
        Err(json!("projectUnavailable"))
    );
}

#[test]
fn arbitrary_paths_and_stale_registrations_cannot_reach_app_server() {
    let (temp_dir, state) = project_state();
    let unregistered = temp_dir.path().join("unregistered");
    fs::create_dir(&unregistered).unwrap();
    let connection = FakeConnection::new([]);
    let result: Result<ProjectThreadPage, _> = execute_thread_command(
        &state,
        Some(connection.clone()),
        unregistered.to_str().unwrap(),
        |project, connection| ThreadService::list_threads(project, connection, None, 0),
    );
    assert_eq!(result, Err(ProjectCommandError::Project));
    assert_eq!(list_recent(&state).unwrap(), Vec::<LocalProject>::new());

    let selected = register_selection(&state, Some(unregistered.clone()))
        .unwrap()
        .unwrap();
    fs::remove_dir(unregistered).unwrap();
    let stale: Result<ProjectThreadPage, _> = execute_thread_command(
        &state,
        Some(connection.clone()),
        &selected.project.id,
        |project, connection| ThreadService::list_threads(project, connection, None, 0),
    );
    assert_eq!(stale, Err(ProjectCommandError::Project));
    assert_eq!(connection.requests(), Vec::<RecordedRequest>::new());
}

fn project_state() -> (TempDir, ProjectState) {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = ProjectState::new(
        temp_dir
            .path()
            .join("settings")
            .join("recent-projects-v1.json"),
    );
    (temp_dir, state)
}

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
fn command_app(
    temp_dir: &TempDir,
    picker_result: Result<Option<PathBuf>, ProjectCommandError>,
    connection: Option<Arc<dyn ConnectionControl>>,
) -> App<MockRuntime> {
    let settings_file = temp_dir
        .path()
        .join("settings")
        .join("recent-projects-v1.json");
    let dialog_state = ProjectDialogState {
        picker: Arc::new(FixedFolderPicker(picker_result)),
    };
    let connection_state = ProjectConnectionState {
        provider: Arc::new(FixedActiveConnectionProvider(connection)),
    };
    let app = mock_builder()
        .invoke_handler(crate::invoke_handler())
        .build(mock_context(noop_assets()))
        .unwrap();
    assert!(app.manage(dialog_state));
    assert!(app.manage(connection_state));
    assert!(app.manage(ProjectState::new(settings_file)));
    app
}

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
fn invoke_project_command(
    webview: &WebviewWindow<MockRuntime>,
    command: &str,
    body: Value,
) -> Result<Value, Value> {
    tauri::test::get_ipc_response(
        webview,
        InvokeRequest {
            cmd: command.to_string(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map(|response| response.deserialize::<Value>().unwrap())
}

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
struct FixedFolderPicker(Result<Option<PathBuf>, ProjectCommandError>);

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
impl FolderPicker for FixedFolderPicker {
    fn pick_folder(&self) -> Result<Option<PathBuf>, ProjectCommandError> {
        self.0.clone()
    }
}

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
struct FixedActiveConnectionProvider(Option<Arc<dyn ConnectionControl>>);

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
impl ActiveConnectionProvider for FixedActiveConnectionProvider {
    fn active_connection(&self) -> Option<Arc<dyn ConnectionControl>> {
        self.0.clone()
    }
}

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
fn app_server_thread(id: &str, cwd: &str) -> Value {
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

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
fn command_thread(id: &str, cwd: &str) -> Value {
    json!({
        "id": id,
        "name": null,
        "preview": "Preview",
        "createdAt": 10,
        "updatedAt": 20,
        "recencyAt": 30,
        "status": "idle",
        "cwd": cwd
    })
}

#[cfg(any(not(windows), feature = "test-tauri-commands"))]
fn expected_thread_requests(cwd: &str) -> Vec<RecordedRequest> {
    vec![
        request(
            "thread/list",
            json!({
                "cwd": cwd,
                "limit": 50,
                "sortKey": "recency_at",
                "sortDirection": "desc",
                "cursor": "cursor-1"
            }),
        ),
        request("thread/start", json!({ "cwd": cwd })),
        request(
            "thread/read",
            json!({ "threadId": "thr-read", "includeTurns": false }),
        ),
    ]
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

    fn request_without_params(&self, _method: &str) -> Result<Value, ConnectionError> {
        unreachable!("project commands never send parameterless requests")
    }
}

fn request(method: &str, params: Value) -> RecordedRequest {
    RecordedRequest {
        method: method.to_string(),
        params,
    }
}
