use std::sync::Arc;

use serde_json::Value;
use serde_json::json;
use thiserror::Error;

use crate::app_server::ConnectionControl;
use crate::app_server::ConnectionError;
use crate::app_server::ConnectionIdentity;
use crate::app_server::event_router::EventRouter;
use crate::app_server::event_router::EventRouterError;
use crate::app_server::event_router::RunEvent;
use crate::app_server::event_router::RunEventStream;
use crate::task::worktree::TaskWorktree;

const MAX_CORRELATION_ID_BYTES: usize = 128;
pub(crate) const MAX_RUN_PROMPT_BYTES: usize = 4 * 1024;

pub(crate) struct CodexRunRequest<'a> {
    pub(crate) run_id: &'a str,
    pub(crate) thread_id: &'a str,
    pub(crate) prompt: &'a str,
    pub(crate) worktree: &'a TaskWorktree,
}

pub(crate) struct ActiveCodexRun {
    runtime_identity: Arc<()>,
    connection_identity: ConnectionIdentity,
    run_id: String,
    thread_id: String,
    turn_id: String,
    events: RunEventStream,
}

impl ActiveCodexRun {
    pub(crate) fn run_id(&self) -> &str {
        &self.run_id
    }

    pub(crate) fn thread_id(&self) -> &str {
        &self.thread_id
    }

    pub(crate) fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub(crate) fn try_next_event(&self) -> Option<RunEvent> {
        self.events.try_recv()
    }

    pub(crate) fn is_active(&self) -> bool {
        self.events.is_active()
    }
}

pub(crate) struct CodexRuntime {
    identity: Arc<()>,
}

impl Default for CodexRuntime {
    fn default() -> Self {
        Self {
            identity: Arc::new(()),
        }
    }
}

impl CodexRuntime {
    pub(crate) fn start_run(
        &self,
        request: CodexRunRequest<'_>,
        connection: Arc<dyn ConnectionControl>,
        event_router: &EventRouter,
    ) -> Result<ActiveCodexRun, CodexRuntimeError> {
        validate_request(&request)?;
        let connection_identity = connection.connection_identity();
        let events = event_router.prepare_run(
            connection_identity.clone(),
            request.run_id,
            request.thread_id,
        )?;
        let response = connection
            .request(
                "turn/start",
                json!({
                    "threadId": request.thread_id,
                    "clientUserMessageId": request.run_id,
                    "input": [{"type": "text", "text": request.prompt}],
                    "cwd": request.worktree.cwd(),
                    "approvalPolicy": "on-request",
                    "approvalsReviewer": "auto_review",
                    "sandboxPolicy": {
                        "type": "workspaceWrite",
                        "writableRoots": [request.worktree.cwd()],
                        "networkAccess": false,
                    },
                }),
            )
            .map_err(map_connection_error)?;
        let turn_id = parse_started_turn(&response).ok_or(CodexRuntimeError::OutcomeUnknown)?;
        event_router
            .bind_turn(&events, turn_id)
            .map_err(|_| CodexRuntimeError::OutcomeUnknown)?;
        Ok(ActiveCodexRun {
            runtime_identity: self.identity.clone(),
            connection_identity,
            run_id: request.run_id.to_string(),
            thread_id: request.thread_id.to_string(),
            turn_id: turn_id.to_string(),
            events,
        })
    }

    pub(crate) fn interrupt_run(
        &self,
        run: &ActiveCodexRun,
        connection: Arc<dyn ConnectionControl>,
    ) -> Result<(), CodexRuntimeError> {
        if !Arc::ptr_eq(&self.identity, &run.runtime_identity)
            || connection.connection_identity() != run.connection_identity
            || !run.is_active()
        {
            return Err(CodexRuntimeError::RunNotActive);
        }
        let response = connection
            .request(
                "turn/interrupt",
                json!({"threadId": run.thread_id, "turnId": run.turn_id}),
            )
            .map_err(map_connection_error)?;
        if response.as_object().is_none_or(|object| !object.is_empty()) {
            return Err(CodexRuntimeError::OutcomeUnknown);
        }
        Ok(())
    }
}

fn validate_request(request: &CodexRunRequest<'_>) -> Result<(), CodexRuntimeError> {
    if !valid_id(request.run_id)
        || !valid_id(request.thread_id)
        || request.prompt.trim().is_empty()
        || request.prompt.len() > MAX_RUN_PROMPT_BYTES
    {
        return Err(CodexRuntimeError::InvalidRequest);
    }
    Ok(())
}

fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_CORRELATION_ID_BYTES
}

fn parse_started_turn(response: &Value) -> Option<&str> {
    let turn = response.get("turn").and_then(Value::as_object)?;
    let turn_id = turn
        .get("id")
        .and_then(Value::as_str)
        .filter(|turn_id| valid_id(turn_id))?;
    if turn.get("status").and_then(Value::as_str) != Some("inProgress") {
        return None;
    }
    Some(turn_id)
}

fn map_connection_error(error: ConnectionError) -> CodexRuntimeError {
    match error {
        ConnectionError::WriteFailed | ConnectionError::Timeout | ConnectionError::Disconnected => {
            CodexRuntimeError::OutcomeUnknown
        }
        ConnectionError::Serialize
        | ConnectionError::TooManyPending
        | ConnectionError::Remote { .. }
        | ConnectionError::RequestIdExhausted => CodexRuntimeError::RequestFailed,
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum CodexRuntimeError {
    #[error("Codex run request is invalid")]
    InvalidRequest,
    #[error("Codex App Server request failed")]
    RequestFailed,
    #[error("Codex run outcome is unknown")]
    OutcomeUnknown,
    #[error("Codex run event routing failed")]
    EventRouting,
    #[error("Codex run is not active on this runtime connection")]
    RunNotActive,
}

impl From<EventRouterError> for CodexRuntimeError {
    fn from(_error: EventRouterError) -> Self {
        Self::EventRouting
    }
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
