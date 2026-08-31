use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde_json::Value;
use serde_json::json;
use thiserror::Error;

use super::artifact::ArtifactError;
use super::artifact::PatchArtifact;
use super::receipt::RunReceipt;
use super::receipt::RunReceiptInput;
use super::receipt::RunReceiptOutcome;
use super::receipt::TestReport;
use super::service::BeginRunResult;
use super::service::TaskService;
use super::service::TaskServiceError;
use super::types::RunRecord;
use super::types::RunStatus;
use super::types::TaskSpec;
use super::worktree::TaskWorktree;
use super::worktree::TaskWorktreeManager;
use super::worktree::WorktreeCleanup;
use super::worktree::WorktreeCleanupFailure;
use super::worktree::WorktreeError;
use crate::app_server::ConnectionControl;
use crate::app_server::ConnectionError;
use crate::app_server::event_router::EventRouter;
use crate::app_server::event_router::RunEventKind;
use crate::project::ResolvedProject;
use crate::runtime::codex::ActiveCodexRun;
use crate::runtime::codex::CodexRunRequest;
use crate::runtime::codex::CodexRuntime;
use crate::runtime::codex::CodexRuntimeError;
use crate::runtime::codex::MAX_RUN_PROMPT_BYTES;

const CODEX_RUNTIME_ID: &str = "codex";
const MAX_CORRELATION_ID_BYTES: usize = 128;
const MAX_RUNTIME_VERSION_BYTES: usize = 128;
// UTF-8 bytes are a conservative upper bound for byte-level model tokens. This deliberately
// rejects some valid prompts rather than risking an over-limit model-visible item.
const MAX_PROMPT_TOKEN_UPPER_BOUND: usize = 1_000;
const RUNTIME_FAILED_MESSAGE: &str = "Codex Runtime reported that the run failed.";
const OUTCOME_UNKNOWN_MESSAGE: &str = "The Codex run outcome could not be verified.";

pub(crate) struct StartLocalCodexRunRequest<'a> {
    pub(crate) project_id: &'a str,
    pub(crate) task_id: &'a str,
    pub(crate) run_id: &'a str,
    pub(crate) node_id: &'a str,
    pub(crate) runtime_version: &'a str,
    pub(crate) project: &'a ResolvedProject,
    pub(crate) connection: Arc<dyn ConnectionControl>,
}

pub(crate) enum LocalCodexRunStart {
    Active(Box<ActiveLocalCodexRun>),
    Existing(Box<RunRecord>),
    Finished(Box<LocalCodexRunCompletion>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LocalCodexRunProgress {
    Pending(RunRecord),
    Finished(LocalCodexRunCompletion),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LocalCodexRunCompletion {
    pub(crate) run: RunRecord,
    pub(crate) patch: PatchArtifact,
    pub(crate) cleanup: WorktreeCleanup,
}

pub(crate) struct LocalCodexRunService {
    tasks: Arc<TaskService>,
    worktrees: TaskWorktreeManager,
    runtime: Arc<CodexRuntime>,
    events: Arc<EventRouter>,
    clock: Arc<dyn TaskRunClock>,
}

impl LocalCodexRunService {
    pub(crate) fn new(
        tasks: Arc<TaskService>,
        worktrees: TaskWorktreeManager,
        events: Arc<EventRouter>,
    ) -> Self {
        Self::with_clock(tasks, worktrees, events, Arc::new(SystemTaskRunClock))
    }

    fn with_clock(
        tasks: Arc<TaskService>,
        worktrees: TaskWorktreeManager,
        events: Arc<EventRouter>,
        clock: Arc<dyn TaskRunClock>,
    ) -> Self {
        Self {
            tasks,
            worktrees,
            runtime: Arc::new(CodexRuntime::default()),
            events,
            clock,
        }
    }

    pub(crate) fn start(
        &self,
        request: StartLocalCodexRunRequest<'_>,
    ) -> Result<LocalCodexRunStart, TaskRunError> {
        validate_metadata(&request)?;
        let task = self
            .tasks
            .get_project_task(request.project_id, request.task_id)?;
        let run = task
            .runs
            .iter()
            .find(|run| run.id == request.run_id)
            .ok_or(TaskRunError::InvalidRequest)?;
        if run.status != RunStatus::Queued {
            return Ok(LocalCodexRunStart::Existing(Box::new(run.clone())));
        }
        let prompt = task_prompt(&task.spec)?;
        let worktree = self.worktrees.create(request.project, request.run_id)?;
        match self.tasks.begin_run(request.task_id, request.run_id)? {
            BeginRunResult::Existing(run) => {
                let _ = worktree.cleanup();
                return Ok(LocalCodexRunStart::Existing(Box::new(run)));
            }
            BeginRunResult::Started(_) => {}
        }
        let started_at = self.clock.now_unix_seconds();
        let metadata = RunMetadata {
            task_id: request.task_id.to_string(),
            run_id: request.run_id.to_string(),
            node_id: request.node_id.to_string(),
            runtime_version: request.runtime_version.to_string(),
            started_at,
        };
        let thread_id = match start_thread(&worktree, request.connection.clone()) {
            Ok(thread_id) => thread_id,
            Err(_) => {
                return self
                    .finish_start_failure(worktree, metadata, RunReceiptOutcome::Failed)
                    .map(|completion| LocalCodexRunStart::Finished(Box::new(completion)));
            }
        };
        let active = match self.runtime.start_run(
            CodexRunRequest {
                run_id: request.run_id,
                thread_id: &thread_id,
                prompt: &prompt,
                worktree: &worktree,
            },
            request.connection.clone(),
            &self.events,
        ) {
            Ok(active) => active,
            Err(error) => {
                let outcome = match error {
                    CodexRuntimeError::OutcomeUnknown => RunReceiptOutcome::OutcomeUnknown,
                    CodexRuntimeError::InvalidRequest
                    | CodexRuntimeError::RequestFailed
                    | CodexRuntimeError::EventRouting
                    | CodexRuntimeError::RunNotActive => RunReceiptOutcome::Failed,
                };
                return self
                    .finish_start_failure(worktree, metadata, outcome)
                    .map(|completion| LocalCodexRunStart::Finished(Box::new(completion)));
            }
        };
        Ok(LocalCodexRunStart::Active(Box::new(ActiveLocalCodexRun {
            tasks: self.tasks.clone(),
            clock: self.clock.clone(),
            runtime: self.runtime.clone(),
            connection: request.connection,
            metadata,
            worktree,
            active,
            pending_outcome: None,
            completion: None,
        })))
    }

    fn finish_start_failure(
        &self,
        worktree: TaskWorktree,
        metadata: RunMetadata,
        outcome: RunReceiptOutcome,
    ) -> Result<LocalCodexRunCompletion, TaskRunError> {
        finish_run(&self.tasks, &self.clock, &worktree, &metadata, outcome)
    }
}

pub(crate) struct ActiveLocalCodexRun {
    tasks: Arc<TaskService>,
    clock: Arc<dyn TaskRunClock>,
    runtime: Arc<CodexRuntime>,
    connection: Arc<dyn ConnectionControl>,
    metadata: RunMetadata,
    worktree: TaskWorktree,
    active: ActiveCodexRun,
    pending_outcome: Option<RunReceiptOutcome>,
    completion: Option<LocalCodexRunCompletion>,
}

impl ActiveLocalCodexRun {
    pub(crate) fn task_id(&self) -> &str {
        &self.metadata.task_id
    }

    pub(crate) fn run_id(&self) -> &str {
        &self.metadata.run_id
    }

    pub(crate) fn interrupt(&self) -> Result<(), TaskRunError> {
        self.runtime
            .interrupt_run(&self.active, self.connection.clone())
            .map_err(Into::into)
    }

    pub(crate) fn poll(&mut self) -> Result<LocalCodexRunProgress, TaskRunError> {
        if let Some(completion) = &self.completion {
            return Ok(LocalCodexRunProgress::Finished(completion.clone()));
        }
        if let Some(outcome) = self.pending_outcome {
            let completion = finish_run(
                &self.tasks,
                &self.clock,
                &self.worktree,
                &self.metadata,
                outcome,
            )?;
            self.completion = Some(completion.clone());
            return Ok(LocalCodexRunProgress::Finished(completion));
        }
        let Some(event) = self.active.try_next_event() else {
            if !self.active.is_active() {
                self.pending_outcome = Some(RunReceiptOutcome::OutcomeUnknown);
                return self.poll();
            }
            return self.current_run().map(LocalCodexRunProgress::Pending);
        };
        match event.kind {
            RunEventKind::Running => {
                let current = self.current_run()?;
                if current.status == RunStatus::WaitingApproval {
                    return self
                        .tasks
                        .transition_run(
                            &self.metadata.task_id,
                            &self.metadata.run_id,
                            RunStatus::Running,
                        )
                        .map(LocalCodexRunProgress::Pending)
                        .map_err(Into::into);
                }
                Ok(LocalCodexRunProgress::Pending(current))
            }
            RunEventKind::WaitingApproval => self
                .tasks
                .transition_run(
                    &self.metadata.task_id,
                    &self.metadata.run_id,
                    RunStatus::WaitingApproval,
                )
                .map(LocalCodexRunProgress::Pending)
                .map_err(Into::into),
            RunEventKind::Completed => self.finish(RunReceiptOutcome::Success),
            RunEventKind::Failed => self.finish(RunReceiptOutcome::Failed),
            RunEventKind::Interrupted => self.finish(RunReceiptOutcome::Cancelled),
            RunEventKind::Gap { .. } => self.finish(RunReceiptOutcome::OutcomeUnknown),
        }
    }

    pub(crate) fn mark_disconnected(&mut self) -> Result<LocalCodexRunProgress, TaskRunError> {
        self.finish(RunReceiptOutcome::OutcomeUnknown)
    }

    fn finish(
        &mut self,
        outcome: RunReceiptOutcome,
    ) -> Result<LocalCodexRunProgress, TaskRunError> {
        self.pending_outcome = Some(outcome);
        self.poll()
    }

    fn current_run(&self) -> Result<RunRecord, TaskRunError> {
        self.tasks
            .get_task(&self.metadata.task_id)?
            .runs
            .into_iter()
            .find(|run| run.id == self.metadata.run_id)
            .ok_or(TaskRunError::InvalidRequest)
    }
}

#[derive(Clone)]
struct RunMetadata {
    task_id: String,
    run_id: String,
    node_id: String,
    runtime_version: String,
    started_at: i64,
}

fn finish_run(
    tasks: &TaskService,
    clock: &Arc<dyn TaskRunClock>,
    worktree: &TaskWorktree,
    metadata: &RunMetadata,
    outcome: RunReceiptOutcome,
) -> Result<LocalCodexRunCompletion, TaskRunError> {
    let patch = PatchArtifact::collect(worktree)?;
    let error = match outcome {
        RunReceiptOutcome::Success | RunReceiptOutcome::Cancelled => None,
        RunReceiptOutcome::Failed => Some(RUNTIME_FAILED_MESSAGE.to_string()),
        RunReceiptOutcome::OutcomeUnknown => Some(OUTCOME_UNKNOWN_MESSAGE.to_string()),
    };
    let receipt = RunReceipt::new(RunReceiptInput {
        task_id: metadata.task_id.clone(),
        run_id: metadata.run_id.clone(),
        node_id: metadata.node_id.clone(),
        runtime_id: CODEX_RUNTIME_ID.to_string(),
        runtime_version: metadata.runtime_version.clone(),
        started_at: metadata.started_at,
        finished_at: clock.now_unix_seconds().max(metadata.started_at),
        outcome,
        summary: None,
        error,
        tests: TestReport::NotReported,
        patch: patch.clone(),
    })
    .map_err(|_| TaskRunError::InvalidRequest)?;
    let run = tasks.finalize_run(receipt)?;
    let cleanup = if outcome == RunReceiptOutcome::OutcomeUnknown || patch.patch.is_none() {
        WorktreeCleanup::Retained {
            reason: WorktreeCleanupFailure::EvidenceIncomplete,
        }
    } else {
        worktree.cleanup()
    };
    Ok(LocalCodexRunCompletion {
        run,
        patch,
        cleanup,
    })
}

fn start_thread(
    worktree: &TaskWorktree,
    connection: Arc<dyn ConnectionControl>,
) -> Result<String, ThreadStartError> {
    let response = connection
        .request(
            "thread/start",
            json!({
                "cwd": worktree.cwd(),
                "config": {"skills.include_instructions": false},
            }),
        )
        .map_err(ThreadStartError::Connection)?;
    let thread = response
        .get("thread")
        .and_then(Value::as_object)
        .ok_or(ThreadStartError::InvalidResponse)?;
    let thread_id = thread
        .get("id")
        .and_then(Value::as_str)
        .filter(|thread_id| valid_id(thread_id))
        .ok_or(ThreadStartError::InvalidResponse)?;
    if response.get("cwd").and_then(Value::as_str) != Some(worktree.cwd())
        || thread.get("cwd").and_then(Value::as_str) != Some(worktree.cwd())
    {
        return Err(ThreadStartError::InvalidResponse);
    }
    Ok(thread_id.to_string())
}

fn task_prompt(spec: &TaskSpec) -> Result<String, TaskRunError> {
    let mut prompt = format!("Goal:\n{}", spec.goal);
    if !spec.constraints.is_empty() {
        prompt.push_str("\n\nConstraints:");
        for constraint in &spec.constraints {
            prompt.push_str("\n- ");
            prompt.push_str(constraint);
        }
    }
    if prompt.len() > MAX_RUN_PROMPT_BYTES || prompt.len() > MAX_PROMPT_TOKEN_UPPER_BOUND {
        return Err(TaskRunError::InvalidRequest);
    }
    Ok(prompt)
}

fn validate_metadata(request: &StartLocalCodexRunRequest<'_>) -> Result<(), TaskRunError> {
    if request.project.id() != request.project_id
        || !valid_id(request.task_id)
        || !valid_id(request.run_id)
        || !valid_id(request.node_id)
        || request.runtime_version.trim().is_empty()
        || request.runtime_version.len() > MAX_RUNTIME_VERSION_BYTES
    {
        return Err(TaskRunError::InvalidRequest);
    }
    Ok(())
}

fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_CORRELATION_ID_BYTES
}

trait TaskRunClock: Send + Sync {
    fn now_unix_seconds(&self) -> i64;
}

struct SystemTaskRunClock;

impl TaskRunClock for SystemTaskRunClock {
    fn now_unix_seconds(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .try_into()
            .unwrap_or(i64::MAX)
    }
}

#[derive(Debug, Error)]
enum ThreadStartError {
    #[error("Codex thread start failed")]
    Connection(#[source] ConnectionError),
    #[error("Codex thread start response was invalid")]
    InvalidResponse,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum TaskRunError {
    #[error("local Codex run request is invalid")]
    InvalidRequest,
    #[error("local Task state is unavailable")]
    TaskState,
    #[error("local Task worktree is unavailable")]
    Worktree,
    #[error("local Task artifact is unavailable")]
    Artifact,
    #[error("Codex Runtime request failed")]
    Runtime,
}

impl From<TaskServiceError> for TaskRunError {
    fn from(_error: TaskServiceError) -> Self {
        Self::TaskState
    }
}

impl From<WorktreeError> for TaskRunError {
    fn from(_error: WorktreeError) -> Self {
        Self::Worktree
    }
}

impl From<ArtifactError> for TaskRunError {
    fn from(_error: ArtifactError) -> Self {
        Self::Artifact
    }
}

impl From<CodexRuntimeError> for TaskRunError {
    fn from(_error: CodexRuntimeError) -> Self {
        Self::Runtime
    }
}

#[cfg(test)]
#[path = "orchestrator_tests.rs"]
mod tests;
