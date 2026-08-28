use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use serde_json::json;
#[cfg(test)]
use tauri::Url;

use crate::account::login::LoginStartResponse;
use crate::account::login::UrlOpener;
use crate::account::login::is_cancel_confirmation;
use crate::account::login::parse_login_response;
use crate::account::login::parse_official_auth_url;
use crate::account::types::AccountStatus;
use crate::app_server::ConnectionControl;
use crate::app_server::ConnectionError;
use crate::app_server::ConnectionIdentity;

pub(crate) use self::commands::AccountCommand;
use self::login_completion::LoginCompletionState;
use self::login_completion::StartedAttemptDisposition;
use self::login_completion::TaskSpawner;
use self::login_completion::ThreadTaskSpawner;
use self::read::parse_account_status;
pub(crate) use self::status_observer::AccountStatusObserver;
#[cfg(test)]
use self::status_observer::NoopAccountStatusObserver;

const ACCOUNT_UNAVAILABLE_MESSAGE: &str = "账号状态暂时不可用。";
const BROWSER_OPEN_MESSAGE: &str = "无法打开 ChatGPT 登录页面，请重试。";
const LOGIN_UNAVAILABLE_MESSAGE: &str = "ChatGPT 登录暂时不可用，请重试。";
const UNSUPPORTED_ACCOUNT_MESSAGE: &str = "当前核心服务配置不支持 ChatGPT 账号登录。";

#[derive(Clone)]
pub(crate) struct AccountService {
    inner: Arc<AccountServiceInner>,
}

struct AccountServiceInner {
    browser_open_operation: Mutex<()>,
    login_operation: Mutex<()>,
    published_status: Mutex<Option<AccountStatus>>,
    state: Mutex<AccountServiceState>,
    status_observer: Arc<dyn AccountStatusObserver>,
    task_spawner: Arc<dyn TaskSpawner>,
    url_opener: Arc<dyn UrlOpener>,
}

struct AccountServiceState {
    connection: Option<Arc<dyn ConnectionControl>>,
    connection_identity: Option<ConnectionIdentity>,
    connection_revision: u64,
    login_completion: LoginCompletionState,
    login_attempt: Option<LoginAttempt>,
    refresh_revision: u64,
    status: AccountStatus,
}

#[derive(Clone)]
struct LoginAttempt {
    login_id: String,
}

#[cfg(test)]
struct UnavailableUrlOpener;

#[cfg(test)]
impl UrlOpener for UnavailableUrlOpener {
    fn open(&self, _url: &Url) -> Result<(), ()> {
        Err(())
    }
}

impl AccountService {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_url_opener(Arc::new(UnavailableUrlOpener))
    }

    #[cfg(test)]
    pub(crate) fn with_url_opener(url_opener: Arc<dyn UrlOpener>) -> Self {
        Self::with_dependencies(url_opener, Arc::new(ThreadTaskSpawner))
    }

    pub(crate) fn with_runtime_dependencies(
        url_opener: Arc<dyn UrlOpener>,
        status_observer: Arc<dyn AccountStatusObserver>,
    ) -> Self {
        Self::with_all_dependencies(url_opener, Arc::new(ThreadTaskSpawner), status_observer)
    }

    #[cfg(test)]
    fn with_dependencies(
        url_opener: Arc<dyn UrlOpener>,
        task_spawner: Arc<dyn TaskSpawner>,
    ) -> Self {
        Self::with_all_dependencies(
            url_opener,
            task_spawner,
            Arc::new(NoopAccountStatusObserver),
        )
    }

    fn with_all_dependencies(
        url_opener: Arc<dyn UrlOpener>,
        task_spawner: Arc<dyn TaskSpawner>,
        status_observer: Arc<dyn AccountStatusObserver>,
    ) -> Self {
        Self {
            inner: Arc::new(AccountServiceInner {
                browser_open_operation: Mutex::new(()),
                login_operation: Mutex::new(()),
                published_status: Mutex::new(None),
                state: Mutex::new(AccountServiceState {
                    connection: None,
                    connection_identity: None,
                    connection_revision: 0,
                    login_completion: LoginCompletionState::default(),
                    login_attempt: None,
                    refresh_revision: 0,
                    status: AccountStatus::Checking,
                }),
                status_observer,
                task_spawner,
                url_opener,
            }),
        }
    }

    pub(crate) fn connect(&self, connection: Arc<dyn ConnectionControl>) -> AccountStatus {
        let connection_identity = connection.connection_identity();
        let _browser_open = self
            .inner
            .browser_open_operation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let status = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            state.connection_revision = state.connection_revision.wrapping_add(1);
            state.connection = Some(connection);
            state.connection_identity = Some(connection_identity);
            state.login_completion.reset();
            state.login_attempt = None;
            state.status = AccountStatus::Checking;
            state.status.clone()
        };
        self.publish_status(&status);
        status
    }

    pub(crate) fn disconnect(&self) -> AccountStatus {
        let _browser_open = self
            .inner
            .browser_open_operation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let status = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            state.connection_revision = state.connection_revision.wrapping_add(1);
            state.connection = None;
            state.connection_identity = None;
            state.login_completion.reset();
            state.login_attempt = None;
            state.status = retryable_account_error();
            state.status.clone()
        };
        self.publish_status(&status);
        status
    }

    pub(crate) fn status(&self) -> AccountStatus {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .status
            .clone()
    }

    pub(crate) fn refresh(&self) -> AccountStatus {
        let (connection, connection_revision, refresh_revision) = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            state.refresh_revision = state.refresh_revision.wrapping_add(1);
            (
                state.connection.clone(),
                state.connection_revision,
                state.refresh_revision,
            )
        };

        let next_status = match connection {
            Some(connection) => {
                match connection.request("account/read", json!({ "refreshToken": false })) {
                    Ok(result) => parse_account_status(result),
                    Err(
                        ConnectionError::Serialize
                        | ConnectionError::WriteFailed
                        | ConnectionError::Timeout
                        | ConnectionError::TooManyPending
                        | ConnectionError::Disconnected
                        | ConnectionError::Remote { .. }
                        | ConnectionError::RequestIdExhausted,
                    ) => retryable_account_error(),
                }
            }
            None => retryable_account_error(),
        };

        let status = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.connection_revision == connection_revision
                && state.refresh_revision == refresh_revision
                && state.login_attempt.is_none()
            {
                state.status = next_status;
            }
            state.status.clone()
        };
        self.publish_status(&status);
        status
    }

    pub(crate) fn start_browser_login(&self) -> AccountStatus {
        let _operation = self
            .inner
            .login_operation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (connection, connection_revision) = match self.prepare_login() {
            Ok(prepared) => prepared,
            Err(status) => return status,
        };
        let response = connection
            .request(
                "account/login/start",
                json!({
                    "type": "chatgpt",
                    "useHostedLoginSuccessPage": false,
                }),
            )
            .ok()
            .and_then(parse_login_response);
        let (login_id, auth_url) = match response {
            Some(LoginStartResponse::Chatgpt { login_id, auth_url }) => (login_id, auth_url),
            Some(LoginStartResponse::ChatgptDeviceCode { login_id, .. })
                if !login_id.is_empty() =>
            {
                self.discard_login_start(connection_revision);
                return self.cancel_started_attempt(
                    &connection,
                    connection_revision,
                    LoginAttempt { login_id },
                    login_unavailable_error(),
                );
            }
            Some(LoginStartResponse::ChatgptDeviceCode { .. })
            | Some(LoginStartResponse::Unsupported)
            | None => {
                self.discard_login_start(connection_revision);
                return self
                    .set_status_for_connection(connection_revision, login_unavailable_error());
            }
        };
        if login_id.is_empty() {
            self.discard_login_start(connection_revision);
            return self.set_status_for_connection(connection_revision, login_unavailable_error());
        }
        let attempt = LoginAttempt { login_id };
        let Some(auth_url) = parse_official_auth_url(&auth_url) else {
            self.discard_login_start(connection_revision);
            return self.cancel_started_attempt(
                &connection,
                connection_revision,
                attempt,
                browser_open_error(),
            );
        };
        let browser_open = self
            .inner
            .browser_open_operation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match self.finish_login_start_with_attempt(
            connection_revision,
            attempt.clone(),
            AccountStatus::BrowserPending,
        ) {
            StartedAttemptDisposition::Installed => {}
            StartedAttemptDisposition::Completed => {
                drop(browser_open);
                return self.status();
            }
            StartedAttemptDisposition::Stale => {
                let status = self.status();
                drop(browser_open);
                return self.cancel_started_attempt(
                    &connection,
                    connection_revision,
                    attempt,
                    status,
                );
            }
        }
        if self.inner.url_opener.open(&auth_url).is_err() {
            drop(browser_open);
            return self.cancel_started_attempt(
                &connection,
                connection_revision,
                attempt,
                browser_open_error(),
            );
        }
        let status = self.status();
        drop(browser_open);
        status
    }

    fn prepare_login(&self) -> Result<(Arc<dyn ConnectionControl>, u64), AccountStatus> {
        self.cancel_active_attempt()?;
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(connection) = state.connection.clone() else {
            state.refresh_revision = state.refresh_revision.wrapping_add(1);
            state.status = login_unavailable_error();
            return Err(state.status.clone());
        };
        let connection_revision = state.connection_revision;
        state
            .login_completion
            .begin_login_start(connection_revision);
        Ok((connection, connection_revision))
    }

    fn cancel_active_attempt(&self) -> Result<(), AccountStatus> {
        let (connection, connection_revision, login_id) = {
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let Some(attempt) = &state.login_attempt else {
                return Ok(());
            };
            let Some(connection) = state.connection.clone() else {
                return Err(state.status.clone());
            };
            (
                connection,
                state.connection_revision,
                attempt.login_id.clone(),
            )
        };
        if self.cancel_login(&connection, &login_id).is_err() {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.connection_revision == connection_revision
                && state.connection.is_some()
                && state
                    .login_attempt
                    .as_ref()
                    .is_some_and(|attempt| attempt.login_id == login_id)
            {
                state.refresh_revision = state.refresh_revision.wrapping_add(1);
                state.status = login_unavailable_error();
            }
            return Err(state.status.clone());
        }
        self.clear_attempt(&login_id);
        Ok(())
    }

    fn cancel_login(
        &self,
        connection: &Arc<dyn ConnectionControl>,
        login_id: &str,
    ) -> Result<(), ()> {
        let response = connection
            .request("account/login/cancel", json!({ "loginId": login_id }))
            .map_err(|_| ())?;
        is_cancel_confirmation(response).then_some(()).ok_or(())
    }

    fn set_status_for_connection(
        &self,
        connection_revision: u64,
        status: AccountStatus,
    ) -> AccountStatus {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.connection_revision == connection_revision && state.connection.is_some() {
            state.refresh_revision = state.refresh_revision.wrapping_add(1);
            state.status = status;
        }
        state.status.clone()
    }
}

mod account_actions;
mod commands;
mod login_completion;
mod read;
mod status_observer;

fn retryable_account_error() -> AccountStatus {
    AccountStatus::Error {
        message: ACCOUNT_UNAVAILABLE_MESSAGE.to_string(),
        retryable: true,
    }
}

fn login_unavailable_error() -> AccountStatus {
    AccountStatus::Error {
        message: LOGIN_UNAVAILABLE_MESSAGE.to_string(),
        retryable: true,
    }
}

fn browser_open_error() -> AccountStatus {
    AccountStatus::Error {
        message: BROWSER_OPEN_MESSAGE.to_string(),
        retryable: true,
    }
}

fn unsupported_account_error() -> AccountStatus {
    AccountStatus::Error {
        message: UNSUPPORTED_ACCOUNT_MESSAGE.to_string(),
        retryable: false,
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "service/account_actions_tests.rs"]
mod account_actions_tests;

#[cfg(test)]
#[path = "service/login_completion_tests.rs"]
mod login_completion_tests;

#[cfg(test)]
#[path = "service/commands_tests.rs"]
mod commands_tests;

#[cfg(test)]
#[path = "service/status_observer_tests.rs"]
mod status_observer_tests;
