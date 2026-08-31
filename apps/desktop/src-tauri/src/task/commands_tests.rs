#![cfg(any(not(windows), feature = "test-tauri-commands"))]

use std::sync::Arc;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tauri::Manager;
use tauri::WebviewWindow;
use tauri::WebviewWindowBuilder;
use tauri::ipc::CallbackFn;
use tauri::ipc::InvokeBody;
use tauri::test::MockRuntime;
use tauri::test::mock_builder;
use tauri::test::mock_context;
use tauri::test::noop_assets;
use tauri::webview::InvokeRequest;

use super::*;

#[test]
fn tauri_task_commands_are_registered_and_return_only_sanitized_errors() {
    let temp_dir = tempfile::tempdir().unwrap();
    let app = mock_builder()
        .invoke_handler(crate::invoke_handler())
        .build(mock_context(noop_assets()))
        .unwrap();
    let state = TaskRunState::new(
        temp_dir.path().join("tasks-v1.json"),
        temp_dir.path().join("worktrees"),
        Arc::new(EventRouter::default()),
        Arc::new(|_update| {}),
    )
    .unwrap();
    assert!(app.manage(state));
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let project_id = format!("project-v1-{}", "a".repeat(64));

    assert_eq!(
        invoke_task_command(
            &webview,
            "list_local_tasks",
            json!({"projectId": project_id})
        ),
        Ok(json!([]))
    );
    assert_eq!(
        invoke_task_command(
            &webview,
            "start_local_task",
            json!({
                "projectId": project_id,
                "idempotencyKey": "request-1",
                "goal": "goal",
                "constraints": []
            })
        ),
        Err(json!("projectUnavailable"))
    );
    assert_eq!(
        invoke_task_command(
            &webview,
            "stop_local_task",
            json!({
                "projectId": project_id,
                "taskId": "task-v1-missing",
                "runId": "run-v1-missing"
            })
        ),
        Err(json!("runUnavailable"))
    );
}

fn invoke_task_command(
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
