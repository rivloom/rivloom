use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use tauri::Runtime;

use super::local_state::LocalTaskError;
use super::local_state::LocalTaskRun;
use super::local_state::StartLocalTaskRequest;
use super::local_state::TaskRunState;
use super::types::TaskRecord;
use crate::app_server::ConnectionControl;
use crate::app_server::event_router::EventRouter;
use crate::app_server::log_diagnostic;
use crate::app_server::state::AppServerState;
use crate::identity::IdentityService;
use crate::project::ProjectState;
use crate::runtime_status::RuntimeStatus;

const TASK_RUN_CHANGED_EVENT: &str = "task-run-changed";

pub(crate) fn create_task_state<R: Runtime>(
    app_handle: AppHandle<R>,
    tasks_file: PathBuf,
    worktree_root: PathBuf,
    events: Arc<EventRouter>,
) -> Result<TaskRunState, LocalTaskError> {
    let event_handle = app_handle.clone();
    TaskRunState::new(
        tasks_file,
        worktree_root,
        events,
        Arc::new(move |update| {
            if let Err(error) = event_handle.emit_to("main", TASK_RUN_CHANGED_EVENT, update) {
                log_diagnostic("task event failed", &error.to_string());
            }
        }),
    )
}

#[tauri::command]
pub(crate) async fn list_local_tasks<R: Runtime>(
    app_handle: AppHandle<R>,
    project_id: String,
) -> Result<Vec<TaskRecord>, TaskCommandError> {
    run_blocking(move || {
        task_state(&app_handle)?
            .list(&project_id)
            .map_err(map_local_error)
    })
    .await
}

#[tauri::command]
pub(crate) async fn start_local_task<R: Runtime>(
    app_handle: AppHandle<R>,
    project_id: String,
    idempotency_key: String,
    goal: String,
    constraints: Vec<String>,
) -> Result<LocalTaskRun, TaskCommandError> {
    run_blocking(move || {
        let project = app_handle
            .try_state::<ProjectState>()
            .ok_or(TaskCommandError::ProjectUnavailable)?
            .lookup_project(&project_id)
            .map_err(|_| TaskCommandError::ProjectUnavailable)?;
        let (connection, runtime_version) = runtime_details(&app_handle)?;
        let node_id = app_handle
            .try_state::<IdentityService>()
            .ok_or(TaskCommandError::IdentityUnavailable)?
            .get()
            .map_err(|_| TaskCommandError::IdentityUnavailable)?
            .device_id;
        task_state(&app_handle)?
            .start(StartLocalTaskRequest {
                project_id: &project_id,
                idempotency_key: &idempotency_key,
                goal: &goal,
                constraints: &constraints,
                node_id: &node_id,
                runtime_version: &runtime_version,
                project: &project,
                connection,
            })
            .map_err(map_local_error)
    })
    .await
}

#[tauri::command]
pub(crate) async fn stop_local_task<R: Runtime>(
    app_handle: AppHandle<R>,
    project_id: String,
    task_id: String,
    run_id: String,
) -> Result<TaskRecord, TaskCommandError> {
    run_blocking(move || {
        task_state(&app_handle)?
            .stop(&project_id, &task_id, &run_id)
            .map_err(map_local_error)
    })
    .await
}

async fn run_blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, TaskCommandError> + Send + 'static,
) -> Result<T, TaskCommandError> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .unwrap_or(Err(TaskCommandError::TaskUnavailable))
}

fn task_state<R: Runtime>(
    app_handle: &AppHandle<R>,
) -> Result<tauri::State<'_, TaskRunState>, TaskCommandError> {
    app_handle
        .try_state::<TaskRunState>()
        .ok_or(TaskCommandError::TaskUnavailable)
}

fn runtime_details<R: Runtime>(
    app_handle: &AppHandle<R>,
) -> Result<(Arc<dyn ConnectionControl>, String), TaskCommandError> {
    let state = app_handle
        .try_state::<AppServerState>()
        .ok_or(TaskCommandError::RuntimeUnavailable)?;
    let runtime_version = match state.current_status() {
        RuntimeStatus::Connected { app_version, .. } => app_version,
        RuntimeStatus::Starting | RuntimeStatus::Error { .. } | RuntimeStatus::Stopped => {
            return Err(TaskCommandError::RuntimeUnavailable);
        }
    };
    let connection = state
        .active_connection()
        .ok_or(TaskCommandError::RuntimeUnavailable)?;
    Ok((connection, runtime_version))
}

fn map_local_error(error: LocalTaskError) -> TaskCommandError {
    match error {
        LocalTaskError::InvalidRequest => TaskCommandError::InvalidTask,
        LocalTaskError::State => TaskCommandError::TaskUnavailable,
        LocalTaskError::Runtime => TaskCommandError::RuntimeUnavailable,
        LocalTaskError::RunNotActive => TaskCommandError::RunUnavailable,
        LocalTaskError::Busy => TaskCommandError::CapacityReached,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum TaskCommandError {
    #[serde(rename = "invalidTask")]
    InvalidTask,
    #[serde(rename = "taskUnavailable")]
    TaskUnavailable,
    #[serde(rename = "projectUnavailable")]
    ProjectUnavailable,
    #[serde(rename = "identityUnavailable")]
    IdentityUnavailable,
    #[serde(rename = "runtimeUnavailable")]
    RuntimeUnavailable,
    #[serde(rename = "runUnavailable")]
    RunUnavailable,
    #[serde(rename = "taskCapacityReached")]
    CapacityReached,
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
