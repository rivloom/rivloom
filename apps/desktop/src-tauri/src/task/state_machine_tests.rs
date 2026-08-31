use pretty_assertions::assert_eq;
use serde_json::json;

use super::state_machine::StateMachineError;
use super::types::*;

#[test]
fn every_legal_task_transition_updates_the_entire_record() {
    let transitions = [
        (TaskStatus::Draft, TaskStatus::Offered),
        (TaskStatus::Draft, TaskStatus::Cancelled),
        (TaskStatus::Offered, TaskStatus::Accepted),
        (TaskStatus::Offered, TaskStatus::Cancelled),
        (TaskStatus::Accepted, TaskStatus::Running),
        (TaskStatus::Accepted, TaskStatus::Cancelled),
        (TaskStatus::Accepted, TaskStatus::Failed),
        (TaskStatus::Running, TaskStatus::AwaitingReview),
        (TaskStatus::Running, TaskStatus::Cancelled),
        (TaskStatus::Running, TaskStatus::Failed),
        (TaskStatus::Running, TaskStatus::OutcomeUnknown),
        (TaskStatus::AwaitingReview, TaskStatus::Approved),
        (TaskStatus::AwaitingReview, TaskStatus::Rejected),
    ];

    for (from, to) in transitions {
        let mut actual = task_in(from);
        let mut expected = actual.clone();
        expected.status = to;
        expected.events.push(TaskEvent {
            sequence: 1,
            kind: TaskEventKind::TaskStatusChanged { from, to },
        });

        actual.transition(to, TransitionDetails::default()).unwrap();

        assert_eq!(actual, expected);
    }
}

#[test]
fn every_legal_run_transition_updates_the_entire_record() {
    let transitions = [
        (RunStatus::Queued, RunStatus::Running),
        (RunStatus::Queued, RunStatus::Cancelled),
        (RunStatus::Queued, RunStatus::Failed),
        (RunStatus::Running, RunStatus::WaitingApproval),
        (RunStatus::Running, RunStatus::Completed),
        (RunStatus::Running, RunStatus::Cancelled),
        (RunStatus::Running, RunStatus::Failed),
        (RunStatus::Running, RunStatus::OutcomeUnknown),
        (RunStatus::WaitingApproval, RunStatus::Running),
        (RunStatus::WaitingApproval, RunStatus::Completed),
        (RunStatus::WaitingApproval, RunStatus::Cancelled),
        (RunStatus::WaitingApproval, RunStatus::Failed),
        (RunStatus::WaitingApproval, RunStatus::OutcomeUnknown),
    ];

    for (from, to) in transitions {
        let mut actual = task_with_run(from);
        let mut expected = actual.clone();
        expected.runs[0].status = to;
        expected.events.push(TaskEvent {
            sequence: 1,
            kind: TaskEventKind::RunStatusChanged {
                run_id: "run-1".to_string(),
                from,
                to,
            },
        });

        actual
            .transition_run("run-1", to, TransitionDetails::default())
            .unwrap();

        assert_eq!(actual, expected);
    }
}

#[test]
fn invalid_duplicate_and_unknown_transitions_leave_state_unchanged() {
    let mut draft = task_in(TaskStatus::Draft);
    let original = draft.clone();
    assert_eq!(
        draft.transition(TaskStatus::Running, TransitionDetails::default()),
        Err(StateMachineError::InvalidTaskTransition)
    );
    assert_eq!(draft, original);

    let mut completed = task_with_run(RunStatus::Completed);
    let original = completed.clone();
    assert_eq!(
        completed.transition_run("run-1", RunStatus::Completed, TransitionDetails::default()),
        Err(StateMachineError::InvalidRunTransition)
    );
    assert_eq!(
        completed.transition_run(
            "missing-run",
            RunStatus::Running,
            TransitionDetails::default()
        ),
        Err(StateMachineError::UnknownRun)
    );
    assert_eq!(completed, original);
}

#[test]
fn running_disconnect_becomes_unknown_and_never_requeues() {
    let mut task = task_with_run(RunStatus::Running);
    task.transition_run(
        "run-1",
        RunStatus::OutcomeUnknown,
        TransitionDetails::with_error("connection lost"),
    )
    .unwrap();
    assert_eq!(task.runs[0].status, RunStatus::OutcomeUnknown);
    assert_eq!(task.runs[0].error.as_deref(), Some("connection lost"));

    let unknown = task.clone();
    assert_eq!(
        task.transition_run("run-1", RunStatus::Queued, TransitionDetails::default()),
        Err(StateMachineError::InvalidRunTransition)
    );
    assert_eq!(task, unknown);
}

#[test]
fn goal_constraints_summary_error_and_events_have_hard_limits() {
    assert_eq!(
        TaskRecord::new(
            "task-1",
            TaskSpec::new("x".repeat(MAX_GOAL_BYTES + 1), vec![])
        ),
        Err(StateMachineError::GoalTooLong)
    );
    assert_eq!(
        TaskRecord::new(
            "task-1",
            TaskSpec::new("goal", vec!["x".repeat(MAX_CONSTRAINT_BYTES + 1)])
        ),
        Err(StateMachineError::ConstraintTooLong)
    );
    assert_eq!(
        TaskRecord::new(
            "task-1",
            TaskSpec::new("goal", vec!["x".to_string(); MAX_CONSTRAINTS + 1])
        ),
        Err(StateMachineError::TooManyConstraints)
    );
    assert_eq!(
        TaskRecord::new(
            "task-1",
            TaskSpec::new(
                "goal",
                vec![
                    "x".repeat(MAX_CONSTRAINT_BYTES);
                    MAX_CONSTRAINT_TOTAL_BYTES / MAX_CONSTRAINT_BYTES + 1
                ]
            )
        ),
        Err(StateMachineError::ConstraintsTooLong)
    );

    let mut task = task_in(TaskStatus::Running);
    let original = task.clone();
    assert_eq!(
        task.transition(
            TaskStatus::AwaitingReview,
            TransitionDetails::with_summary("x".repeat(MAX_SUMMARY_BYTES + 1))
        ),
        Err(StateMachineError::SummaryTooLong)
    );
    assert_eq!(task, original);
    assert_eq!(
        task.transition(
            TaskStatus::Failed,
            TransitionDetails::with_error("x".repeat(MAX_ERROR_BYTES + 1))
        ),
        Err(StateMachineError::ErrorTooLong)
    );
    assert_eq!(task, original);

    task.events = vec![task_event(); MAX_EVENTS];
    let full = task.clone();
    assert_eq!(
        task.transition(TaskStatus::Failed, TransitionDetails::default()),
        Err(StateMachineError::EventLimitReached)
    );
    assert_eq!(task, full);
}

#[test]
fn run_registration_is_bounded_and_rejects_duplicates() {
    let mut task = task_in(TaskStatus::Accepted);
    task.register_run("run-1").unwrap();
    let expected = task.clone();

    assert_eq!(
        task.register_run("run-1"),
        Err(StateMachineError::DuplicateRun)
    );
    assert_eq!(task, expected);
}

#[test]
fn task_records_serialize_to_the_exact_frontend_contract() {
    let mut task = task_with_run(RunStatus::WaitingApproval);
    task.status = TaskStatus::Running;
    task.events.push(TaskEvent {
        sequence: 1,
        kind: TaskEventKind::RunStatusChanged {
            run_id: "run-1".to_string(),
            from: RunStatus::Running,
            to: RunStatus::WaitingApproval,
        },
    });

    assert_eq!(
        serde_json::to_value(task).unwrap(),
        json!({
            "id": "task-1",
            "spec": {"goal": "goal", "constraints": ["stay bounded"]},
            "status": "running",
            "summary": null,
            "error": null,
            "runs": [{
                "id": "run-1",
                "status": "waitingApproval",
                "summary": null,
                "error": null,
            }],
            "events": [{
                "sequence": 1,
                "kind": {
                    "type": "runStatusChanged",
                    "runId": "run-1",
                    "from": "running",
                    "to": "waitingApproval",
                },
            }],
        })
    );
}

fn task_in(status: TaskStatus) -> TaskRecord {
    TaskRecord {
        id: "task-1".to_string(),
        spec: TaskSpec::new("goal", vec!["stay bounded".to_string()]),
        status,
        summary: None,
        error: None,
        runs: vec![],
        events: vec![],
    }
}

fn task_with_run(status: RunStatus) -> TaskRecord {
    let mut task = task_in(TaskStatus::Accepted);
    task.runs.push(RunRecord {
        id: "run-1".to_string(),
        status,
        summary: None,
        error: None,
    });
    task
}

fn task_event() -> TaskEvent {
    TaskEvent {
        sequence: 1,
        kind: TaskEventKind::TaskStatusChanged {
            from: TaskStatus::Draft,
            to: TaskStatus::Offered,
        },
    }
}
