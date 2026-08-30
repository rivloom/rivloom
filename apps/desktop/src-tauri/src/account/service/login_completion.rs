use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::PoisonError;
use std::thread;

use serde_json::Value;

use super::AccountService;
use super::LoginAttempt;
use crate::account::types::CodexRuntimeAuthStatus;
use crate::app_server::ConnectionControl;
use crate::app_server::ConnectionIdentity;
use crate::app_server::NotificationObserver;

type BackgroundTask = Box<dyn FnOnce() + Send + 'static>;

pub(super) const MAX_EARLY_COMPLETION_IDS: usize = 8;
pub(super) const MAX_EARLY_COMPLETION_BYTES: usize = 4 * 1024;

/// Starts short backend tasks without making notification callbacks wait for RPC work.
///
/// Implementations must return promptly. A task may perform blocking App Server requests after the
/// callback has returned, and must be executed at most once.
pub(crate) trait TaskSpawner: Send + Sync {
    fn spawn(&self, task: BackgroundTask) -> Result<(), ()>;
}

pub(super) struct ThreadTaskSpawner;

impl TaskSpawner for ThreadTaskSpawner {
    fn spawn(&self, task: BackgroundTask) -> Result<(), ()> {
        thread::Builder::new()
            .name("rivloom-account-refresh".to_string())
            .spawn(task)
            .map(|_| ())
            .map_err(|_| ())
    }
}

#[derive(Default)]
pub(super) struct LoginCompletionState {
    login_start: Option<LoginStartState>,
    refresh: BackgroundRefreshState,
}

impl LoginCompletionState {
    pub(super) fn reset(&mut self) {
        self.login_start = None;
        self.refresh = BackgroundRefreshState::Idle;
    }

    pub(super) fn begin_login_start(&mut self, connection_revision: u64) {
        self.login_start = Some(LoginStartState {
            connection_revision,
            completion_bytes: 0,
            completion_ids: VecDeque::new(),
        });
    }

    fn remember_early_completion(&mut self, connection_revision: u64, login_id: &str) {
        let Some(login_start) = &mut self.login_start else {
            return;
        };
        if login_start.connection_revision != connection_revision
            || login_id.is_empty()
            || login_id.len() > MAX_EARLY_COMPLETION_BYTES
        {
            return;
        }
        if let Some(index) = login_start
            .completion_ids
            .iter()
            .position(|completed_id| completed_id == login_id)
            && let Some(existing) = login_start.completion_ids.remove(index)
        {
            login_start.completion_bytes -= existing.len();
        }
        while login_start.completion_ids.len() >= MAX_EARLY_COMPLETION_IDS
            || login_start.completion_bytes > MAX_EARLY_COMPLETION_BYTES - login_id.len()
        {
            let Some(evicted) = login_start.completion_ids.pop_front() else {
                break;
            };
            login_start.completion_bytes -= evicted.len();
        }
        login_start.completion_ids.push_back(login_id.to_string());
        login_start.completion_bytes += login_id.len();
    }

    fn discard_login_start(&mut self, connection_revision: u64) {
        if self
            .login_start
            .as_ref()
            .is_some_and(|login_start| login_start.connection_revision == connection_revision)
        {
            self.login_start = None;
        }
    }

    fn take_matching_early_completion(&mut self, connection_revision: u64, login_id: &str) -> bool {
        let Some(login_start) = self
            .login_start
            .take_if(|login_start| login_start.connection_revision == connection_revision)
        else {
            return false;
        };
        login_start
            .completion_ids
            .iter()
            .any(|completed_id| completed_id == login_id)
    }
}

struct LoginStartState {
    connection_revision: u64,
    completion_bytes: usize,
    completion_ids: VecDeque<String>,
}

pub(super) enum StartedAttemptDisposition {
    Installed,
    Completed,
    Stale,
}

#[derive(Default)]
enum BackgroundRefreshState {
    #[default]
    Idle,
    Scheduled,
    Running {
        requested: bool,
    },
}

impl NotificationObserver for AccountService {
    fn on_notification(
        &self,
        connection_identity: &ConnectionIdentity,
        method: &str,
        params: &Value,
    ) {
        match method {
            "account/login/completed" => {
                self.handle_login_completed(connection_identity, params);
            }
            "account/updated" => {
                self.schedule_current_background_refresh(connection_identity);
            }
            _ => {}
        }
    }
}

impl AccountService {
    pub(super) fn discard_login_start(&self, connection_revision: u64) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state
            .login_completion
            .discard_login_start(connection_revision);
    }

    pub(super) fn finish_login_start_with_attempt(
        &self,
        connection_revision: u64,
        attempt: LoginAttempt,
        status: CodexRuntimeAuthStatus,
    ) -> StartedAttemptDisposition {
        let disposition = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.connection_revision != connection_revision || state.connection.is_none() {
                state
                    .login_completion
                    .discard_login_start(connection_revision);
                StartedAttemptDisposition::Stale
            } else if state
                .login_completion
                .take_matching_early_completion(connection_revision, &attempt.login_id)
            {
                state.login_attempt = None;
                state.refresh_revision = state.refresh_revision.wrapping_add(1);
                state.status = CodexRuntimeAuthStatus::Checking;
                StartedAttemptDisposition::Completed
            } else {
                state.login_attempt = Some(attempt);
                state.refresh_revision = state.refresh_revision.wrapping_add(1);
                state.status = status;
                StartedAttemptDisposition::Installed
            }
        };
        if matches!(disposition, StartedAttemptDisposition::Completed) {
            self.schedule_background_refresh(connection_revision);
        }
        disposition
    }

    pub(super) fn install_attempt(
        &self,
        connection_revision: u64,
        attempt: LoginAttempt,
        status: CodexRuntimeAuthStatus,
    ) -> bool {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.connection_revision != connection_revision || state.connection.is_none() {
            return false;
        }
        state.login_attempt = Some(attempt);
        state.refresh_revision = state.refresh_revision.wrapping_add(1);
        state.status = status;
        true
    }

    pub(super) fn cancel_started_attempt(
        &self,
        connection: &Arc<dyn ConnectionControl>,
        connection_revision: u64,
        attempt: LoginAttempt,
        status: CodexRuntimeAuthStatus,
    ) -> CodexRuntimeAuthStatus {
        if self.cancel_login(connection, &attempt.login_id).is_ok() {
            self.clear_attempt(&attempt.login_id);
            return self.set_status_for_connection(connection_revision, status);
        }
        if self.install_attempt(connection_revision, attempt, status) {
            return self.status();
        }
        self.status()
    }

    pub(super) fn clear_attempt(&self, login_id: &str) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state
            .login_attempt
            .as_ref()
            .is_some_and(|attempt| attempt.login_id == login_id)
        {
            state.login_attempt = None;
            state.refresh_revision = state.refresh_revision.wrapping_add(1);
            state.status = CodexRuntimeAuthStatus::Checking;
        }
    }

    fn handle_login_completed(&self, connection_identity: &ConnectionIdentity, params: &Value) {
        let Some(params) = params.as_object() else {
            return;
        };
        let Some(login_id) = params.get("loginId").and_then(Value::as_str) else {
            return;
        };
        if params.get("success").and_then(Value::as_bool).is_none()
            || !matches!(
                params.get("error"),
                None | Some(Value::Null | Value::String(_))
            )
        {
            return;
        }
        let refresh_revision = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.connection_identity.as_ref() != Some(connection_identity) {
                return;
            }
            if state
                .login_attempt
                .as_ref()
                .is_some_and(|attempt| attempt.login_id == login_id)
            {
                state.login_attempt = None;
                state.refresh_revision = state.refresh_revision.wrapping_add(1);
                state.status = CodexRuntimeAuthStatus::Checking;
                Some(state.connection_revision)
            } else {
                let connection_revision = state.connection_revision;
                state
                    .login_completion
                    .remember_early_completion(connection_revision, login_id);
                None
            }
        };
        if let Some(connection_revision) = refresh_revision {
            self.schedule_background_refresh(connection_revision);
        }
    }

    fn schedule_current_background_refresh(&self, connection_identity: &ConnectionIdentity) {
        let connection_revision = {
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.connection_identity.as_ref() != Some(connection_identity) {
                return;
            }
            state.connection_revision
        };
        self.schedule_background_refresh(connection_revision);
    }

    pub(super) fn schedule_background_refresh(&self, connection_revision: u64) {
        let should_spawn = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.connection_revision != connection_revision || state.connection.is_none() {
                return;
            }
            match &mut state.login_completion.refresh {
                refresh @ BackgroundRefreshState::Idle => {
                    *refresh = BackgroundRefreshState::Scheduled;
                    true
                }
                BackgroundRefreshState::Scheduled => false,
                BackgroundRefreshState::Running { requested } => {
                    *requested = true;
                    false
                }
            }
        };
        if should_spawn {
            self.spawn_background_refresh(connection_revision);
        }
    }

    fn spawn_background_refresh(&self, connection_revision: u64) {
        let service = self.clone();
        if self
            .inner
            .task_spawner
            .spawn(Box::new(move || {
                service.run_background_refresh(connection_revision);
            }))
            .is_err()
        {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.connection_revision == connection_revision
                && matches!(
                    state.login_completion.refresh,
                    BackgroundRefreshState::Scheduled
                )
            {
                state.login_completion.refresh = BackgroundRefreshState::Idle;
            }
        }
    }

    fn run_background_refresh(&self, connection_revision: u64) {
        let should_refresh = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.connection_revision != connection_revision || state.connection.is_none() {
                false
            } else if matches!(
                state.login_completion.refresh,
                BackgroundRefreshState::Scheduled
            ) {
                state.login_completion.refresh =
                    BackgroundRefreshState::Running { requested: false };
                true
            } else {
                false
            }
        };
        if !should_refresh {
            return;
        }

        self.refresh();

        let should_repeat = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.connection_revision != connection_revision || state.connection.is_none() {
                return;
            }
            match state.login_completion.refresh {
                BackgroundRefreshState::Running { requested: true } => {
                    state.login_completion.refresh = BackgroundRefreshState::Scheduled;
                    true
                }
                BackgroundRefreshState::Running { requested: false }
                | BackgroundRefreshState::Scheduled
                | BackgroundRefreshState::Idle => {
                    state.login_completion.refresh = BackgroundRefreshState::Idle;
                    false
                }
            }
        };
        if should_repeat {
            self.spawn_background_refresh(connection_revision);
        }
    }
}
