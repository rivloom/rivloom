use thiserror::Error;

use super::types::*;

impl TaskRecord {
    pub(crate) fn new(id: impl Into<String>, spec: TaskSpec) -> Result<Self, StateMachineError> {
        let id = id.into();
        validate_id(&id, StateMachineError::InvalidTaskId)?;
        validate_spec(&spec)?;
        Ok(Self {
            id,
            spec,
            status: TaskStatus::Draft,
            summary: None,
            error: None,
            runs: vec![],
            events: vec![],
        })
    }

    pub(crate) fn transition(
        &mut self,
        next: TaskStatus,
        details: TransitionDetails,
    ) -> Result<(), StateMachineError> {
        let from = self.status;
        if !valid_task_transition(from, next) {
            return Err(StateMachineError::InvalidTaskTransition);
        }
        validate_details(&details)?;
        self.ensure_event_capacity()?;
        self.status = next;
        apply_details(&mut self.summary, &mut self.error, details);
        self.push_event(TaskEventKind::TaskStatusChanged { from, to: next });
        Ok(())
    }

    pub(crate) fn register_run(
        &mut self,
        run_id: impl Into<String>,
    ) -> Result<(), StateMachineError> {
        let run_id = run_id.into();
        validate_id(&run_id, StateMachineError::InvalidRunId)?;
        if self.status != TaskStatus::Accepted {
            return Err(StateMachineError::RunRegistrationNotAllowed);
        }
        if self.runs.iter().any(|run| run.id == run_id) {
            return Err(StateMachineError::DuplicateRun);
        }
        self.ensure_event_capacity()?;
        self.runs.push(RunRecord {
            id: run_id.clone(),
            status: RunStatus::Queued,
            summary: None,
            error: None,
        });
        self.push_event(TaskEventKind::RunRegistered { run_id });
        Ok(())
    }

    pub(crate) fn transition_run(
        &mut self,
        run_id: &str,
        next: RunStatus,
        details: TransitionDetails,
    ) -> Result<(), StateMachineError> {
        let run_index = self
            .runs
            .iter()
            .position(|run| run.id == run_id)
            .ok_or(StateMachineError::UnknownRun)?;
        let from = self.runs[run_index].status;
        if !valid_run_transition(from, next) {
            return Err(StateMachineError::InvalidRunTransition);
        }
        validate_details(&details)?;
        self.ensure_event_capacity()?;
        let run = &mut self.runs[run_index];
        run.status = next;
        apply_details(&mut run.summary, &mut run.error, details);
        self.push_event(TaskEventKind::RunStatusChanged {
            run_id: run_id.to_string(),
            from,
            to: next,
        });
        Ok(())
    }

    fn ensure_event_capacity(&self) -> Result<(), StateMachineError> {
        if self.events.len() >= MAX_EVENTS {
            Err(StateMachineError::EventLimitReached)
        } else {
            Ok(())
        }
    }

    fn push_event(&mut self, kind: TaskEventKind) {
        self.events.push(TaskEvent {
            sequence: self.events.len() as u32 + 1,
            kind,
        });
    }
}

fn validate_spec(spec: &TaskSpec) -> Result<(), StateMachineError> {
    if spec.goal.trim().is_empty() {
        return Err(StateMachineError::InvalidGoal);
    }
    if spec.goal.len() > MAX_GOAL_BYTES {
        return Err(StateMachineError::GoalTooLong);
    }
    if spec.constraints.len() > MAX_CONSTRAINTS {
        return Err(StateMachineError::TooManyConstraints);
    }
    let mut total_bytes = 0;
    for constraint in &spec.constraints {
        if constraint.trim().is_empty() {
            return Err(StateMachineError::InvalidConstraint);
        }
        if constraint.len() > MAX_CONSTRAINT_BYTES {
            return Err(StateMachineError::ConstraintTooLong);
        }
        total_bytes += constraint.len();
    }
    if total_bytes > MAX_CONSTRAINT_TOTAL_BYTES {
        return Err(StateMachineError::ConstraintsTooLong);
    }
    Ok(())
}

fn validate_id(value: &str, error: StateMachineError) -> Result<(), StateMachineError> {
    if value.trim().is_empty() || value.len() > MAX_ID_BYTES {
        Err(error)
    } else {
        Ok(())
    }
}

fn validate_details(details: &TransitionDetails) -> Result<(), StateMachineError> {
    if details
        .summary
        .as_ref()
        .is_some_and(|summary| summary.len() > MAX_SUMMARY_BYTES)
    {
        return Err(StateMachineError::SummaryTooLong);
    }
    if details
        .error
        .as_ref()
        .is_some_and(|error| error.len() > MAX_ERROR_BYTES)
    {
        return Err(StateMachineError::ErrorTooLong);
    }
    Ok(())
}

fn apply_details(
    summary: &mut Option<String>,
    error: &mut Option<String>,
    details: TransitionDetails,
) {
    if let Some(next_summary) = details.summary {
        *summary = Some(next_summary);
    }
    if let Some(next_error) = details.error {
        *error = Some(next_error);
    }
}

fn valid_task_transition(from: TaskStatus, to: TaskStatus) -> bool {
    matches!(
        (from, to),
        (
            TaskStatus::Draft,
            TaskStatus::Offered | TaskStatus::Cancelled
        ) | (
            TaskStatus::Offered,
            TaskStatus::Accepted | TaskStatus::Cancelled
        ) | (
            TaskStatus::Accepted,
            TaskStatus::Running | TaskStatus::Cancelled | TaskStatus::Failed
        ) | (
            TaskStatus::Running,
            TaskStatus::AwaitingReview
                | TaskStatus::Cancelled
                | TaskStatus::Failed
                | TaskStatus::OutcomeUnknown
        ) | (
            TaskStatus::AwaitingReview,
            TaskStatus::Approved | TaskStatus::Rejected
        )
    )
}

fn valid_run_transition(from: RunStatus, to: RunStatus) -> bool {
    matches!(
        (from, to),
        (
            RunStatus::Queued,
            RunStatus::Running | RunStatus::Cancelled | RunStatus::Failed
        ) | (
            RunStatus::Running,
            RunStatus::WaitingApproval
                | RunStatus::Completed
                | RunStatus::Cancelled
                | RunStatus::Failed
                | RunStatus::OutcomeUnknown
        ) | (
            RunStatus::WaitingApproval,
            RunStatus::Running
                | RunStatus::Completed
                | RunStatus::Cancelled
                | RunStatus::Failed
                | RunStatus::OutcomeUnknown
        )
    )
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum StateMachineError {
    #[error("task id is invalid")]
    InvalidTaskId,
    #[error("run id is invalid")]
    InvalidRunId,
    #[error("task goal is empty")]
    InvalidGoal,
    #[error("task goal is too long")]
    GoalTooLong,
    #[error("task has too many constraints")]
    TooManyConstraints,
    #[error("task constraint is empty")]
    InvalidConstraint,
    #[error("task constraint is too long")]
    ConstraintTooLong,
    #[error("task constraints are too long")]
    ConstraintsTooLong,
    #[error("summary is too long")]
    SummaryTooLong,
    #[error("error detail is too long")]
    ErrorTooLong,
    #[error("task transition is invalid")]
    InvalidTaskTransition,
    #[error("run transition is invalid")]
    InvalidRunTransition,
    #[error("run registration is not allowed in the current task state")]
    RunRegistrationNotAllowed,
    #[error("run already exists")]
    DuplicateRun,
    #[error("run does not exist")]
    UnknownRun,
    #[error("task event limit reached")]
    EventLimitReached,
}
