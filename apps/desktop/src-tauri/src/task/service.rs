use std::sync::Mutex;

use thiserror::Error;

use super::receipt::RunReceipt;
use super::receipt::RunReceiptOutcome;
use super::state_machine::StateMachineError;
use super::storage::StorageError;
use super::storage::StoredRunKey;
use super::storage::StoredTask;
use super::storage::TaskStore;
use super::storage::normalize_tasks;
use super::storage::valid_idempotency_key;
use super::storage::valid_project_id;
use super::types::RunRecord;
use super::types::RunStatus;
use super::types::TaskRecord;
use super::types::TaskSpec;
use super::types::TaskStatus;
use super::types::TransitionDetails;

const RESTARTED_RUN_ERROR: &str =
    "Rivloom restarted before the local run outcome could be verified.";

pub(super) struct CreateTaskRequest {
    pub(super) task_id: String,
    pub(super) idempotency_key: String,
    pub(super) project_id: String,
    pub(super) spec: TaskSpec,
}

pub(super) struct RegisterRunRequest {
    pub(super) task_id: String,
    pub(super) run_id: String,
    pub(super) idempotency_key: String,
    pub(super) project_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum BeginRunResult {
    Started(RunRecord),
    Existing(RunRecord),
}

pub(super) struct TaskService {
    store: TaskStore,
    tasks: Mutex<Option<Vec<StoredTask>>>,
}

impl TaskService {
    pub(super) fn new(store: TaskStore) -> Self {
        Self {
            store,
            tasks: Mutex::new(None),
        }
    }

    pub(super) fn create_task(
        &self,
        request: CreateTaskRequest,
    ) -> Result<TaskRecord, TaskServiceError> {
        if !valid_idempotency_key(&request.idempotency_key) {
            return Err(TaskServiceError::InvalidIdempotencyKey);
        }
        if !valid_project_id(&request.project_id) {
            return Err(TaskServiceError::InvalidProjectId);
        }
        let mut cache = self.tasks.lock().map_err(|_| TaskServiceError::State)?;
        self.ensure_loaded(&mut cache)?;
        let tasks = cache.as_ref().ok_or(TaskServiceError::State)?;
        if let Some(existing) = tasks
            .iter()
            .find(|task| task.idempotency_key == request.idempotency_key)
        {
            if existing.project_id.as_deref() != Some(request.project_id.as_str()) {
                return Err(TaskServiceError::IdempotencyConflict);
            }
            return Ok(existing.record.clone());
        }
        let record = TaskRecord::new(request.task_id, request.spec)?;
        let mut next = tasks.clone();
        next.insert(
            0,
            StoredTask {
                idempotency_key: request.idempotency_key,
                project_id: Some(request.project_id),
                record: record.clone(),
                run_keys: vec![],
            },
        );
        let next = normalize_tasks(next);
        self.store.save(&next)?;
        *cache = Some(next);
        Ok(record)
    }

    pub(super) fn list_tasks(&self, project_id: &str) -> Result<Vec<TaskRecord>, TaskServiceError> {
        if !valid_project_id(project_id) {
            return Err(TaskServiceError::InvalidProjectId);
        }
        let mut cache = self.tasks.lock().map_err(|_| TaskServiceError::State)?;
        self.ensure_loaded(&mut cache)?;
        cache
            .as_ref()
            .map(|tasks| {
                tasks
                    .iter()
                    .filter(|task| task.project_id.as_deref() == Some(project_id))
                    .map(|task| task.record.clone())
                    .collect()
            })
            .ok_or(TaskServiceError::State)
    }

    pub(super) fn get_task(&self, task_id: &str) -> Result<TaskRecord, TaskServiceError> {
        let mut cache = self.tasks.lock().map_err(|_| TaskServiceError::State)?;
        self.ensure_loaded(&mut cache)?;
        cache
            .as_ref()
            .and_then(|tasks| tasks.iter().find(|task| task.record.id == task_id))
            .map(|task| task.record.clone())
            .ok_or(TaskServiceError::TaskNotFound)
    }

    pub(super) fn get_project_task(
        &self,
        project_id: &str,
        task_id: &str,
    ) -> Result<TaskRecord, TaskServiceError> {
        if !valid_project_id(project_id) {
            return Err(TaskServiceError::InvalidProjectId);
        }
        let mut cache = self.tasks.lock().map_err(|_| TaskServiceError::State)?;
        self.ensure_loaded(&mut cache)?;
        cache
            .as_ref()
            .and_then(|tasks| {
                tasks.iter().find(|task| {
                    task.project_id.as_deref() == Some(project_id) && task.record.id == task_id
                })
            })
            .map(|task| task.record.clone())
            .ok_or(TaskServiceError::TaskNotFound)
    }

    pub(super) fn accept_task(
        &self,
        project_id: &str,
        task_id: &str,
    ) -> Result<TaskRecord, TaskServiceError> {
        if !valid_project_id(project_id) {
            return Err(TaskServiceError::InvalidProjectId);
        }
        let mut cache = self.tasks.lock().map_err(|_| TaskServiceError::State)?;
        self.ensure_loaded(&mut cache)?;
        let tasks = cache.as_ref().ok_or(TaskServiceError::State)?;
        let task_index = tasks
            .iter()
            .position(|task| {
                task.project_id.as_deref() == Some(project_id) && task.record.id == task_id
            })
            .ok_or(TaskServiceError::TaskNotFound)?;
        let mut next = tasks.clone();
        match next[task_index].record.status {
            TaskStatus::Draft => {
                next[task_index]
                    .record
                    .transition(TaskStatus::Offered, TransitionDetails::default())?;
                next[task_index]
                    .record
                    .transition(TaskStatus::Accepted, TransitionDetails::default())?;
            }
            TaskStatus::Offered => next[task_index]
                .record
                .transition(TaskStatus::Accepted, TransitionDetails::default())?,
            TaskStatus::Accepted
            | TaskStatus::Running
            | TaskStatus::AwaitingReview
            | TaskStatus::Approved
            | TaskStatus::Rejected
            | TaskStatus::Cancelled
            | TaskStatus::Failed
            | TaskStatus::OutcomeUnknown => return Ok(next[task_index].record.clone()),
        }
        self.store.save(&next)?;
        let accepted = next[task_index].record.clone();
        *cache = Some(next);
        Ok(accepted)
    }

    pub(super) fn register_run(
        &self,
        request: RegisterRunRequest,
    ) -> Result<RunRecord, TaskServiceError> {
        if !valid_idempotency_key(&request.idempotency_key) {
            return Err(TaskServiceError::InvalidIdempotencyKey);
        }
        if !valid_project_id(&request.project_id) {
            return Err(TaskServiceError::InvalidProjectId);
        }
        let mut cache = self.tasks.lock().map_err(|_| TaskServiceError::State)?;
        self.ensure_loaded(&mut cache)?;
        let tasks = cache.as_ref().ok_or(TaskServiceError::State)?;
        let task_index = tasks
            .iter()
            .position(|task| {
                task.project_id.as_deref() == Some(request.project_id.as_str())
                    && task.record.id == request.task_id
            })
            .ok_or(TaskServiceError::TaskNotFound)?;
        if let Some(existing) = tasks[task_index]
            .run_keys
            .iter()
            .find(|run| run.idempotency_key == request.idempotency_key)
        {
            return tasks[task_index]
                .record
                .runs
                .iter()
                .find(|run| run.id == existing.run_id)
                .cloned()
                .ok_or(TaskServiceError::Storage);
        }
        let mut next = tasks.clone();
        next[task_index].record.register_run(&request.run_id)?;
        next[task_index].run_keys.push(StoredRunKey {
            idempotency_key: request.idempotency_key,
            run_id: request.run_id.clone(),
        });
        self.store.save(&next)?;
        let run = next[task_index]
            .record
            .runs
            .iter()
            .find(|run| run.id == request.run_id)
            .cloned()
            .ok_or(TaskServiceError::State)?;
        *cache = Some(next);
        Ok(run)
    }

    pub(super) fn begin_run(
        &self,
        task_id: &str,
        run_id: &str,
    ) -> Result<BeginRunResult, TaskServiceError> {
        let mut cache = self.tasks.lock().map_err(|_| TaskServiceError::State)?;
        self.ensure_loaded(&mut cache)?;
        let tasks = cache.as_ref().ok_or(TaskServiceError::State)?;
        let task_index = tasks
            .iter()
            .position(|task| task.record.id == task_id)
            .ok_or(TaskServiceError::TaskNotFound)?;
        let run = tasks[task_index]
            .record
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .ok_or(TaskServiceError::StateMachine)?;
        if run.status != RunStatus::Queued {
            return Ok(BeginRunResult::Existing(run.clone()));
        }
        if tasks[task_index].record.status != TaskStatus::Accepted {
            return Err(TaskServiceError::StateMachine);
        }
        let mut next = tasks.clone();
        next[task_index]
            .record
            .transition(TaskStatus::Running, TransitionDetails::default())?;
        next[task_index].record.transition_run(
            run_id,
            RunStatus::Running,
            TransitionDetails::default(),
        )?;
        self.store.save(&next)?;
        let run = next[task_index]
            .record
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or(TaskServiceError::State)?;
        *cache = Some(next);
        Ok(BeginRunResult::Started(run))
    }

    pub(super) fn transition_run(
        &self,
        task_id: &str,
        run_id: &str,
        status: RunStatus,
    ) -> Result<RunRecord, TaskServiceError> {
        let mut cache = self.tasks.lock().map_err(|_| TaskServiceError::State)?;
        self.ensure_loaded(&mut cache)?;
        let tasks = cache.as_ref().ok_or(TaskServiceError::State)?;
        let task_index = tasks
            .iter()
            .position(|task| task.record.id == task_id)
            .ok_or(TaskServiceError::TaskNotFound)?;
        let current = tasks[task_index]
            .record
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .ok_or(TaskServiceError::StateMachine)?;
        if current.status == status {
            return Ok(current.clone());
        }
        let mut next = tasks.clone();
        next[task_index]
            .record
            .transition_run(run_id, status, TransitionDetails::default())?;
        self.store.save(&next)?;
        let run = next[task_index]
            .record
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or(TaskServiceError::State)?;
        *cache = Some(next);
        Ok(run)
    }

    pub(super) fn finalize_run(&self, receipt: RunReceipt) -> Result<RunRecord, TaskServiceError> {
        receipt
            .verify()
            .map_err(|_| TaskServiceError::StateMachine)?;
        let mut cache = self.tasks.lock().map_err(|_| TaskServiceError::State)?;
        self.ensure_loaded(&mut cache)?;
        let tasks = cache.as_ref().ok_or(TaskServiceError::State)?;
        let task_index = tasks
            .iter()
            .position(|task| task.record.id == receipt.task_id)
            .ok_or(TaskServiceError::TaskNotFound)?;
        let run = tasks[task_index]
            .record
            .runs
            .iter()
            .find(|run| run.id == receipt.run_id)
            .ok_or(TaskServiceError::StateMachine)?;
        if let Some(existing) = &run.receipt {
            return if existing == &receipt {
                Ok(run.clone())
            } else {
                Err(TaskServiceError::StateMachine)
            };
        }
        let (run_status, task_status) = match receipt.outcome {
            RunReceiptOutcome::Success => (RunStatus::Completed, TaskStatus::AwaitingReview),
            RunReceiptOutcome::Failed => (RunStatus::Failed, TaskStatus::Failed),
            RunReceiptOutcome::Cancelled => (RunStatus::Cancelled, TaskStatus::Cancelled),
            RunReceiptOutcome::OutcomeUnknown => {
                (RunStatus::OutcomeUnknown, TaskStatus::OutcomeUnknown)
            }
        };
        let details = TransitionDetails {
            summary: receipt.summary.clone(),
            error: receipt.error.clone(),
        };
        let mut next = tasks.clone();
        next[task_index]
            .record
            .transition_run(&receipt.run_id, run_status, details.clone())?;
        next[task_index]
            .record
            .attach_receipt(&receipt.run_id, receipt.clone())?;
        next[task_index].record.transition(task_status, details)?;
        self.store.save(&next)?;
        let run = next[task_index]
            .record
            .runs
            .iter()
            .find(|run| run.id == receipt.run_id)
            .cloned()
            .ok_or(TaskServiceError::State)?;
        *cache = Some(next);
        Ok(run)
    }

    pub(super) fn reconcile_incomplete_runs(&self) -> Result<Vec<TaskRecord>, TaskServiceError> {
        let mut cache = self.tasks.lock().map_err(|_| TaskServiceError::State)?;
        self.ensure_loaded(&mut cache)?;
        let tasks = cache.as_ref().ok_or(TaskServiceError::State)?;
        let mut next = tasks.clone();
        let mut reconciled = Vec::new();
        for task in &mut next {
            if task.record.status != TaskStatus::Running {
                continue;
            }
            let active_run_ids = task
                .record
                .runs
                .iter()
                .filter(|run| matches!(run.status, RunStatus::Running | RunStatus::WaitingApproval))
                .map(|run| run.id.clone())
                .collect::<Vec<_>>();
            for run_id in active_run_ids {
                task.record.transition_run(
                    &run_id,
                    RunStatus::OutcomeUnknown,
                    TransitionDetails::with_error(RESTARTED_RUN_ERROR),
                )?;
            }
            task.record.transition(
                TaskStatus::OutcomeUnknown,
                TransitionDetails::with_error(RESTARTED_RUN_ERROR),
            )?;
            reconciled.push(task.record.clone());
        }
        if reconciled.is_empty() {
            return Ok(reconciled);
        }
        self.store.save(&next)?;
        *cache = Some(next);
        Ok(reconciled)
    }

    pub(super) fn abandon_run(
        &self,
        task_id: &str,
        run_id: &str,
        error: &str,
    ) -> Result<RunRecord, TaskServiceError> {
        let mut cache = self.tasks.lock().map_err(|_| TaskServiceError::State)?;
        self.ensure_loaded(&mut cache)?;
        let tasks = cache.as_ref().ok_or(TaskServiceError::State)?;
        let task_index = tasks
            .iter()
            .position(|task| task.record.id == task_id)
            .ok_or(TaskServiceError::TaskNotFound)?;
        let mut next = tasks.clone();
        let details = TransitionDetails::with_error(error);
        next[task_index].record.transition_run(
            run_id,
            RunStatus::OutcomeUnknown,
            details.clone(),
        )?;
        next[task_index]
            .record
            .transition(TaskStatus::OutcomeUnknown, details)?;
        self.store.save(&next)?;
        let run = next[task_index]
            .record
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or(TaskServiceError::State)?;
        *cache = Some(next);
        Ok(run)
    }

    fn ensure_loaded(&self, cache: &mut Option<Vec<StoredTask>>) -> Result<(), TaskServiceError> {
        if cache.is_none() {
            *cache = Some(self.store.load()?);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(super) enum TaskServiceError {
    #[error("task state is unavailable")]
    State,
    #[error("task storage is unavailable")]
    Storage,
    #[error("task does not exist")]
    TaskNotFound,
    #[error("idempotency key is invalid")]
    InvalidIdempotencyKey,
    #[error("idempotency key belongs to another local project")]
    IdempotencyConflict,
    #[error("project id is invalid")]
    InvalidProjectId,
    #[error("task or run state is invalid")]
    StateMachine,
}

impl From<StorageError> for TaskServiceError {
    fn from(_error: StorageError) -> Self {
        Self::Storage
    }
}

impl From<StateMachineError> for TaskServiceError {
    fn from(_error: StateMachineError) -> Self {
        Self::StateMachine
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
