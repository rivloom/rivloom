use serde::Deserialize;
use serde::Serialize;

pub(crate) const MAX_GOAL_BYTES: usize = 4 * 1024;
pub(crate) const MAX_CONSTRAINTS: usize = 32;
pub(crate) const MAX_CONSTRAINT_BYTES: usize = 1024;
pub(crate) const MAX_CONSTRAINT_TOTAL_BYTES: usize = 8 * 1024;
pub(crate) const MAX_SUMMARY_BYTES: usize = 4 * 1024;
pub(crate) const MAX_ERROR_BYTES: usize = 2 * 1024;
pub(crate) const MAX_EVENTS: usize = 128;
pub(crate) const MAX_ID_BYTES: usize = 128;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskSpec {
    pub(crate) goal: String,
    pub(crate) constraints: Vec<String>,
}

impl TaskSpec {
    pub(crate) fn new(goal: impl Into<String>, constraints: Vec<String>) -> Self {
        Self {
            goal: goal.into(),
            constraints,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TaskStatus {
    Draft,
    Offered,
    Accepted,
    Running,
    AwaitingReview,
    Approved,
    Rejected,
    Cancelled,
    Failed,
    OutcomeUnknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RunStatus {
    Queued,
    Running,
    WaitingApproval,
    Completed,
    Cancelled,
    Failed,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunRecord {
    pub(crate) id: String,
    pub(crate) status: RunStatus,
    pub(crate) summary: Option<String>,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskRecord {
    pub(crate) id: String,
    pub(crate) spec: TaskSpec,
    pub(crate) status: TaskStatus,
    pub(crate) summary: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) runs: Vec<RunRecord>,
    pub(crate) events: Vec<TaskEvent>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskEvent {
    pub(crate) sequence: u32,
    pub(crate) kind: TaskEventKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum TaskEventKind {
    TaskStatusChanged {
        from: TaskStatus,
        to: TaskStatus,
    },
    RunRegistered {
        run_id: String,
    },
    RunStatusChanged {
        run_id: String,
        from: RunStatus,
        to: RunStatus,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TransitionDetails {
    pub(crate) summary: Option<String>,
    pub(crate) error: Option<String>,
}

impl TransitionDetails {
    pub(crate) fn with_summary(summary: impl Into<String>) -> Self {
        Self {
            summary: Some(summary.into()),
            error: None,
        }
    }

    pub(crate) fn with_error(error: impl Into<String>) -> Self {
        Self {
            summary: None,
            error: Some(error.into()),
        }
    }
}
