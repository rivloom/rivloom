use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_dialog::DialogExt;

use super::service::{ProjectSelection, ProjectServiceError, ResolvedProject};
use super::state::ProjectState;
use super::thread_service::{ThreadService, ThreadServiceError};
use super::types::{LocalProject, ProjectThread, ProjectThreadPage};
use crate::app_server::ConnectionControl;
use crate::app_server::state::AppServerState;

pub(crate) struct ProjectDialogState {
    picker: Arc<dyn FolderPicker>,
}

impl ProjectDialogState {
    pub(crate) fn new<R: Runtime>(app_handle: AppHandle<R>) -> Self {
        Self {
            picker: Arc::new(TauriFolderPicker { app_handle }),
        }
    }

    fn pick_folder(&self) -> Result<Option<PathBuf>, ProjectCommandError> {
        self.picker.pick_folder()
    }
}

pub(crate) struct ProjectConnectionState {
    provider: Arc<dyn ActiveConnectionProvider>,
}

impl ProjectConnectionState {
    pub(crate) fn new<R: Runtime>(app_handle: AppHandle<R>) -> Self {
        Self {
            provider: Arc::new(TauriActiveConnectionProvider { app_handle }),
        }
    }

    fn active_connection(&self) -> Option<Arc<dyn ConnectionControl>> {
        self.provider.active_connection()
    }
}

trait ActiveConnectionProvider: Send + Sync {
    fn active_connection(&self) -> Option<Arc<dyn ConnectionControl>>;
}

struct TauriActiveConnectionProvider<R: Runtime> {
    app_handle: AppHandle<R>,
}

impl<R: Runtime> ActiveConnectionProvider for TauriActiveConnectionProvider<R> {
    fn active_connection(&self) -> Option<Arc<dyn ConnectionControl>> {
        self.app_handle
            .try_state::<AppServerState>()
            .and_then(|state| state.active_connection())
    }
}

trait FolderPicker: Send + Sync {
    fn pick_folder(&self) -> Result<Option<PathBuf>, ProjectCommandError>;
}

struct TauriFolderPicker<R: Runtime> {
    app_handle: AppHandle<R>,
}

impl<R: Runtime> FolderPicker for TauriFolderPicker<R> {
    fn pick_folder(&self) -> Result<Option<PathBuf>, ProjectCommandError> {
        self.app_handle
            .dialog()
            .file()
            .blocking_pick_folder()
            .map(|path| path.into_path().map_err(|_| ProjectCommandError::Project))
            .transpose()
    }
}

#[tauri::command]
pub(crate) async fn list_recent_projects<R: Runtime>(
    app_handle: AppHandle<R>,
) -> Result<Vec<LocalProject>, ProjectCommandError> {
    run_blocking(ProjectCommandError::RecentProjects, move || {
        list_recent(project_state(&app_handle)?.inner())
    })
    .await
}

#[tauri::command]
pub(crate) async fn select_project<R: Runtime>(
    app_handle: AppHandle<R>,
) -> Result<Option<ProjectSelection>, ProjectCommandError> {
    run_blocking(ProjectCommandError::Project, move || {
        let state = project_state(&app_handle)?;
        let selected_path = app_handle
            .try_state::<ProjectDialogState>()
            .ok_or(ProjectCommandError::Project)?
            .pick_folder()?;
        register_selection(state.inner(), selected_path)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remove_recent_project<R: Runtime>(
    app_handle: AppHandle<R>,
    project_id: String,
) -> Result<(), ProjectCommandError> {
    run_blocking(ProjectCommandError::RecentProjects, move || {
        remove_recent(project_state(&app_handle)?.inner(), &project_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn list_project_threads<R: Runtime>(
    app_handle: AppHandle<R>,
    project_id: String,
    cursor: Option<String>,
    loaded_count: usize,
) -> Result<ProjectThreadPage, ProjectCommandError> {
    run_blocking(ProjectCommandError::Core, move || {
        run_thread_command(&app_handle, &project_id, |project, connection| {
            ThreadService::list_threads(project, connection, cursor.as_deref(), loaded_count)
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn start_project_thread<R: Runtime>(
    app_handle: AppHandle<R>,
    project_id: String,
) -> Result<ProjectThread, ProjectCommandError> {
    run_blocking(ProjectCommandError::Core, move || {
        run_thread_command(&app_handle, &project_id, ThreadService::start_thread)
    })
    .await
}

#[tauri::command]
pub(crate) async fn read_project_thread<R: Runtime>(
    app_handle: AppHandle<R>,
    project_id: String,
    thread_id: String,
) -> Result<ProjectThread, ProjectCommandError> {
    run_blocking(ProjectCommandError::Core, move || {
        run_thread_command(&app_handle, &project_id, |project, connection| {
            ThreadService::read_thread(project, connection, &thread_id)
        })
    })
    .await
}

async fn run_blocking<T: Send + 'static>(
    join_error: ProjectCommandError,
    operation: impl FnOnce() -> Result<T, ProjectCommandError> + Send + 'static,
) -> Result<T, ProjectCommandError> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .unwrap_or(Err(join_error))
}

fn run_thread_command<R: Runtime, T>(
    app_handle: &AppHandle<R>,
    project_id: &str,
    operation: impl FnOnce(
        &ResolvedProject,
        Arc<dyn ConnectionControl>,
    ) -> Result<T, ThreadServiceError>,
) -> Result<T, ProjectCommandError> {
    let connection = app_handle
        .try_state::<ProjectConnectionState>()
        .and_then(|state| state.active_connection());
    execute_thread_command(
        project_state(app_handle)?.inner(),
        connection,
        project_id,
        operation,
    )
}

fn execute_thread_command<T>(
    state: &ProjectState,
    connection: Option<Arc<dyn ConnectionControl>>,
    project_id: &str,
    operation: impl FnOnce(
        &ResolvedProject,
        Arc<dyn ConnectionControl>,
    ) -> Result<T, ThreadServiceError>,
) -> Result<T, ProjectCommandError> {
    let connection = connection.ok_or(ProjectCommandError::Core)?;
    let project = state
        .lookup_project(project_id)
        .map_err(map_project_error)?;
    operation(&project, connection).map_err(map_thread_error)
}

fn project_state<R: Runtime>(
    app_handle: &AppHandle<R>,
) -> Result<State<'_, ProjectState>, ProjectCommandError> {
    app_handle
        .try_state::<ProjectState>()
        .ok_or(ProjectCommandError::RecentProjects)
}

fn list_recent(state: &ProjectState) -> Result<Vec<LocalProject>, ProjectCommandError> {
    state.list_recent().map_err(map_project_error)
}

fn register_selection(
    state: &ProjectState,
    selected_path: Option<PathBuf>,
) -> Result<Option<ProjectSelection>, ProjectCommandError> {
    state
        .select_project(selected_path)
        .map_err(map_project_error)
}

fn remove_recent(state: &ProjectState, project_id: &str) -> Result<(), ProjectCommandError> {
    state.remove_recent(project_id).map_err(map_project_error)
}

fn map_project_error(error: ProjectServiceError) -> ProjectCommandError {
    match error {
        ProjectServiceError::Storage => ProjectCommandError::RecentProjects,
        ProjectServiceError::InvalidPath
        | ProjectServiceError::NotDirectory
        | ProjectServiceError::Unreadable
        | ProjectServiceError::NonUnicodePath
        | ProjectServiceError::NotFound
        | ProjectServiceError::Unavailable => ProjectCommandError::Project,
    }
}

fn map_thread_error(error: ThreadServiceError) -> ProjectCommandError {
    match error {
        ThreadServiceError::Disconnected | ThreadServiceError::RequestFailed => {
            ProjectCommandError::Core
        }
        ThreadServiceError::InvalidRequest
        | ThreadServiceError::InvalidResponse
        | ThreadServiceError::ProjectMismatch => ProjectCommandError::Thread,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum ProjectCommandError {
    #[serde(rename = "recentProjectsUnavailable")]
    RecentProjects,
    #[serde(rename = "projectUnavailable")]
    Project,
    #[serde(rename = "coreUnavailable")]
    Core,
    #[serde(rename = "threadUnavailable")]
    Thread,
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
