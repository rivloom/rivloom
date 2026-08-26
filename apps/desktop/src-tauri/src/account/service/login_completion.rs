use std::sync::Arc;
use std::sync::PoisonError;
use std::thread;

use serde_json::Value;

use super::AccountService;
use super::LoginAttempt;
use crate::account::types::AccountStatus;
use crate::app_server::ConnectionControl;
use crate::app_server::NotificationObserver;

type BackgroundTask = Box<dyn FnOnce() + Send + 'static>;

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
    refresh: BackgroundRefreshState,
}

impl LoginCompletionState {
    pub(super) fn reset(&mut self) {
        self.refresh = BackgroundRefreshState::Idle;
    }
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
    fn on_notification(&self, method: &str, params: &Value) {
        match method {
            "account/login/completed" => self.handle_login_completed(params),
            "account/updated" => self.schedule_current_background_refresh(),
            _ => {}
        }
    }
}

impl AccountService {
    pub(super) fn install_attempt(
        &self,
        connection_revision: u64,
        attempt: LoginAttempt,
        status: AccountStatus,
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
        status: AccountStatus,
    ) -> AccountStatus {
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
            state.status = AccountStatus::Checking;
        }
    }

    fn handle_login_completed(&self, params: &Value) {
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
            if state.connection.is_none() {
                return;
            }
            if state
                .login_attempt
                .as_ref()
                .is_some_and(|attempt| attempt.login_id == login_id)
            {
                state.login_attempt = None;
                state.refresh_revision = state.refresh_revision.wrapping_add(1);
                state.status = AccountStatus::Checking;
                Some(state.connection_revision)
            } else {
                None
            }
        };
        if let Some(connection_revision) = refresh_revision {
            self.schedule_background_refresh(connection_revision);
        }
    }

    fn schedule_current_background_refresh(&self) {
        let connection_revision = {
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.connection.is_none() {
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
