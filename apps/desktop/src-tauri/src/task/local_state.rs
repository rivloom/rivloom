use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use thiserror::Error;

use super::artifact::PatchArtifact;
use super::orchestrator::ActiveLocalCodexRun;
use super::orchestrator::LocalCodexRunCompletion;
use super::orchestrator::LocalCodexRunProgress;
use super::orchestrator::LocalCodexRunService;
use super::orchestrator::LocalCodexRunStart;
use super::orchestrator::StartLocalCodexRunRequest;
use super::orchestrator::TaskRunError;
use super::service::CreateTaskRequest;
use super::service::RegisterRunRequest;
use super::service::TaskService;
use super::service::TaskServiceError;
use super::storage::TaskStore;
use super::storage::valid_idempotency_key;
use super::types::RunStatus;
use super::types::TaskRecord;
use super::types::TaskSpec;
use super::worktree::TaskWorktreeManager;
use crate::app_server::ConnectionControl;
use crate::app_server::event_router::EventRouter;
use crate::project::ResolvedProject;

const MAX_ACTIVE_RUNS: usize = 32;
const RUN_POLL_INTERVAL: Duration = Duration::from_millis(/*millis*/ 25);
const WORKER_FAILED_MESSAGE: &str = "Local Codex run observation failed.";

type BackgroundTask = Box<dyn FnOnce() + Send + 'static>;
type SpawnTask = dyn Fn(BackgroundTask) -> Result<(), ()> + Send + Sync;
pub(crate) type TaskUpdateCallback = dyn Fn(&LocalTaskUpdate) + Send + Sync;

pub(crate) struct StartLocalTaskRequest<'a> {
    pub(crate) project_id: &'a str,
    pub(crate) idempotency_key: &'a str,
    pub(crate) goal: &'a str,
    pub(crate) constraints: &'a [String],
    pub(crate) node_id: &'a str,
    pub(crate) runtime_version: &'a str,
    pub(crate) project: &'a ResolvedProject,
    pub(crate) connection: Arc<dyn ConnectionControl>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalTaskRun {
    pub(crate) task: TaskRecord,
    pub(crate) run_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalTaskUpdate {
    pub(crate) project_id: String,
    pub(crate) task: TaskRecord,
    pub(crate) patch: Option<PatchArtifact>,
}

pub(crate) struct TaskRunState {
    inner: Arc<TaskRunStateInner>,
}

struct TaskRunStateInner {
    tasks: Arc<TaskService>,
    runner: LocalCodexRunService,
    active: Mutex<HashMap<String, ActiveRunControl>>,
    on_update: Arc<TaskUpdateCallback>,
    spawn: Arc<SpawnTask>,
}

#[derive(Clone)]
struct ActiveRunControl {
    project_id: String,
    task_id: String,
    run_id: String,
    stop: Arc<AtomicBool>,
}

impl TaskRunState {
    pub(crate) fn new(
        tasks_file: PathBuf,
        worktree_root: PathBuf,
        events: Arc<EventRouter>,
        on_update: Arc<TaskUpdateCallback>,
    ) -> Result<Self, LocalTaskError> {
        Self::with_spawner(
            tasks_file,
            worktree_root,
            events,
            on_update,
            Arc::new(spawn_worker),
        )
    }

    fn with_spawner(
        tasks_file: PathBuf,
        worktree_root: PathBuf,
        events: Arc<EventRouter>,
        on_update: Arc<TaskUpdateCallback>,
        spawn: Arc<SpawnTask>,
    ) -> Result<Self, LocalTaskError> {
        let tasks = Arc::new(TaskService::new(TaskStore::new(tasks_file)));
        tasks.reconcile_incomplete_runs().map_err(map_task_error)?;
        let runner = LocalCodexRunService::new(
            tasks.clone(),
            TaskWorktreeManager::new(worktree_root),
            events,
        );
        Ok(Self {
            inner: Arc::new(TaskRunStateInner {
                tasks,
                runner,
                active: Mutex::default(),
                on_update,
                spawn,
            }),
        })
    }

    pub(crate) fn list(&self, project_id: &str) -> Result<Vec<TaskRecord>, LocalTaskError> {
        self.inner
            .tasks
            .list_tasks(project_id)
            .map_err(map_task_error)
    }

    pub(crate) fn start(
        &self,
        request: StartLocalTaskRequest<'_>,
    ) -> Result<LocalTaskRun, LocalTaskError> {
        if request.project.id() != request.project_id
            || !valid_idempotency_key(request.idempotency_key)
        {
            return Err(LocalTaskError::InvalidRequest);
        }
        let task_id = derived_id("task", request.idempotency_key);
        let run_id = derived_id("run", request.idempotency_key);
        self.inner
            .tasks
            .create_task(CreateTaskRequest {
                task_id: task_id.clone(),
                idempotency_key: request.idempotency_key.to_string(),
                project_id: request.project_id.to_string(),
                spec: TaskSpec::new(request.goal, request.constraints.to_vec()),
            })
            .map_err(map_task_error)?;
        self.inner
            .tasks
            .accept_task(request.project_id, &task_id)
            .map_err(map_task_error)?;
        let run = self
            .inner
            .tasks
            .register_run(RegisterRunRequest {
                task_id: task_id.clone(),
                run_id: run_id.clone(),
                idempotency_key: derived_id("run-key", request.idempotency_key),
                project_id: request.project_id.to_string(),
            })
            .map_err(map_task_error)?;
        if run.status == RunStatus::Queued && self.active_count()? >= MAX_ACTIVE_RUNS {
            return Err(LocalTaskError::Busy);
        }
        match self
            .inner
            .runner
            .start(StartLocalCodexRunRequest {
                project_id: request.project_id,
                task_id: &task_id,
                run_id: &run_id,
                node_id: request.node_id,
                runtime_version: request.runtime_version,
                project: request.project,
                connection: request.connection,
            })
            .map_err(map_run_error)?
        {
            LocalCodexRunStart::Active(active) => {
                self.spawn_active(request.project_id, *active)?;
            }
            LocalCodexRunStart::Existing(_) => {}
            LocalCodexRunStart::Finished(completion) => {
                publish_completion(&self.inner, request.project_id, &task_id, *completion);
            }
        }
        let task = self
            .inner
            .tasks
            .get_project_task(request.project_id, &task_id)
            .map_err(map_task_error)?;
        Ok(LocalTaskRun { task, run_id })
    }

    pub(crate) fn stop(
        &self,
        project_id: &str,
        task_id: &str,
        run_id: &str,
    ) -> Result<TaskRecord, LocalTaskError> {
        let task = self
            .inner
            .tasks
            .get_project_task(project_id, task_id)
            .map_err(|_| LocalTaskError::RunNotActive)?;
        let run_is_active = task.runs.iter().any(|run| {
            run.id == run_id
                && matches!(run.status, RunStatus::Running | RunStatus::WaitingApproval)
        });
        if !run_is_active {
            return Err(LocalTaskError::RunNotActive);
        }
        let active = self
            .inner
            .active
            .lock()
            .map_err(|_| LocalTaskError::State)?;
        let control = active.get(run_id).ok_or(LocalTaskError::RunNotActive)?;
        if control.project_id != project_id || control.task_id != task_id {
            return Err(LocalTaskError::RunNotActive);
        }
        control.stop.store(true, Ordering::Release);
        Ok(task)
    }

    fn active_count(&self) -> Result<usize, LocalTaskError> {
        self.inner
            .active
            .lock()
            .map(|active| active.len())
            .map_err(|_| LocalTaskError::State)
    }

    fn spawn_active(
        &self,
        project_id: &str,
        active: ActiveLocalCodexRun,
    ) -> Result<(), LocalTaskError> {
        let run_id = active.run_id().to_string();
        let control = ActiveRunControl {
            project_id: project_id.to_string(),
            task_id: active.task_id().to_string(),
            run_id: run_id.clone(),
            stop: Arc::new(AtomicBool::new(false)),
        };
        let mut active_runs = self
            .inner
            .active
            .lock()
            .map_err(|_| LocalTaskError::State)?;
        if active_runs.contains_key(&run_id) {
            return Err(LocalTaskError::Busy);
        }
        active_runs.insert(run_id, control.clone());
        drop(active_runs);
        let holder = Arc::new(Mutex::new(Some(active)));
        let worker_holder = holder.clone();
        let inner = self.inner.clone();
        let worker_control = control.clone();
        let worker = Box::new(move || {
            let active = worker_holder
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some(active) = active {
                run_worker(&inner, active, &worker_control);
            }
        });
        if (self.inner.spawn)(worker).is_err() {
            if let Some(active) = holder.lock().unwrap_or_else(PoisonError::into_inner).take() {
                finish_unknown(&self.inner, active, &control);
            }
            remove_active(&self.inner, &control.run_id);
        }
        Ok(())
    }
}

fn run_worker(
    inner: &Arc<TaskRunStateInner>,
    mut active: ActiveLocalCodexRun,
    control: &ActiveRunControl,
) {
    let mut previous = None;
    let mut interrupt_sent = false;
    loop {
        if control.stop.load(Ordering::Acquire) && !interrupt_sent {
            interrupt_sent = true;
            if active.interrupt().is_err() {
                finish_unknown(inner, active, control);
                break;
            }
        }
        match active.poll() {
            Ok(LocalCodexRunProgress::Pending(run)) => {
                if previous.as_ref() != Some(&run) {
                    publish(inner, control, None);
                    previous = Some(run);
                }
                thread::sleep(RUN_POLL_INTERVAL);
            }
            Ok(LocalCodexRunProgress::Finished(completion)) => {
                publish_completion(inner, &control.project_id, &control.task_id, completion);
                break;
            }
            Err(_) => {
                finish_unknown(inner, active, control);
                break;
            }
        }
    }
    remove_active(inner, &control.run_id);
}

fn finish_unknown(
    inner: &Arc<TaskRunStateInner>,
    mut active: ActiveLocalCodexRun,
    control: &ActiveRunControl,
) {
    match active.mark_disconnected() {
        Ok(LocalCodexRunProgress::Finished(completion)) => {
            publish_completion(inner, &control.project_id, &control.task_id, completion);
        }
        Ok(LocalCodexRunProgress::Pending(_)) | Err(_) => {
            let _ =
                inner
                    .tasks
                    .abandon_run(&control.task_id, &control.run_id, WORKER_FAILED_MESSAGE);
            publish(inner, control, None);
        }
    }
}

fn publish_completion(
    inner: &Arc<TaskRunStateInner>,
    project_id: &str,
    task_id: &str,
    completion: LocalCodexRunCompletion,
) {
    if let Ok(task) = inner.tasks.get_project_task(project_id, task_id) {
        (inner.on_update)(&LocalTaskUpdate {
            project_id: project_id.to_string(),
            task,
            patch: Some(completion.patch),
        });
    }
}

fn publish(
    inner: &Arc<TaskRunStateInner>,
    control: &ActiveRunControl,
    patch: Option<PatchArtifact>,
) {
    if let Ok(task) = inner
        .tasks
        .get_project_task(&control.project_id, &control.task_id)
    {
        (inner.on_update)(&LocalTaskUpdate {
            project_id: control.project_id.clone(),
            task,
            patch,
        });
    }
}

fn remove_active(inner: &TaskRunStateInner, run_id: &str) {
    inner
        .active
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(run_id);
}

fn derived_id(kind: &str, idempotency_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update([0]);
    hasher.update(idempotency_key.as_bytes());
    let digest = hasher.finalize();
    format!("{kind}-v1-{digest:x}")
}

fn spawn_worker(task: BackgroundTask) -> Result<(), ()> {
    thread::Builder::new()
        .name("rivloom-task-run".to_string())
        .spawn(task)
        .map(|_| ())
        .map_err(|_| ())
}

fn map_task_error(error: TaskServiceError) -> LocalTaskError {
    match error {
        TaskServiceError::State | TaskServiceError::Storage => LocalTaskError::State,
        TaskServiceError::TaskNotFound
        | TaskServiceError::InvalidIdempotencyKey
        | TaskServiceError::IdempotencyConflict
        | TaskServiceError::InvalidProjectId
        | TaskServiceError::StateMachine => LocalTaskError::InvalidRequest,
    }
}

fn map_run_error(error: TaskRunError) -> LocalTaskError {
    match error {
        TaskRunError::InvalidRequest => LocalTaskError::InvalidRequest,
        TaskRunError::TaskState => LocalTaskError::State,
        TaskRunError::Worktree | TaskRunError::Artifact | TaskRunError::Runtime => {
            LocalTaskError::Runtime
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum LocalTaskError {
    #[error("local Task request is invalid")]
    InvalidRequest,
    #[error("local Task state is unavailable")]
    State,
    #[error("local Codex Runtime is unavailable")]
    Runtime,
    #[error("local Task run is not active")]
    RunNotActive,
    #[error("too many local Task runs are active")]
    Busy,
}

#[cfg(test)]
#[path = "local_state_tests.rs"]
mod tests;
