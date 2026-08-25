use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use tauri::Url;

use crate::account::login::LoginStartResponse;
use crate::account::login::UrlOpener;
use crate::account::login::is_cancel_confirmation;
use crate::account::login::parse_login_response;
use crate::account::login::parse_official_auth_url;
use crate::account::types::AccountStatus;
use crate::app_server::ConnectionControl;
use crate::app_server::ConnectionError;

const ACCOUNT_UNAVAILABLE_MESSAGE: &str = "账号状态暂时不可用。";
const BROWSER_OPEN_MESSAGE: &str = "无法打开 ChatGPT 登录页面，请尝试设备码登录。";
const LOGIN_UNAVAILABLE_MESSAGE: &str = "ChatGPT 登录暂时不可用，请重试。";
const UNSUPPORTED_ACCOUNT_MESSAGE: &str = "当前核心服务配置不支持 ChatGPT 账号登录。";

#[derive(Clone)]
pub(crate) struct AccountService {
    inner: Arc<AccountServiceInner>,
}

struct AccountServiceInner {
    browser_open_operation: Mutex<()>,
    login_operation: Mutex<()>,
    state: Mutex<AccountServiceState>,
    url_opener: Arc<dyn UrlOpener>,
}

struct AccountServiceState {
    connection: Option<Arc<dyn ConnectionControl>>,
    connection_revision: u64,
    login_attempt: Option<LoginAttempt>,
    refresh_revision: u64,
    status: AccountStatus,
}

#[derive(Clone)]
struct LoginAttempt {
    login_id: String,
}

struct UnavailableUrlOpener;

impl UrlOpener for UnavailableUrlOpener {
    fn open(&self, _url: &Url) -> Result<(), ()> {
        Err(())
    }
}

impl AccountService {
    pub(crate) fn new() -> Self {
        Self::with_url_opener(Arc::new(UnavailableUrlOpener))
    }

    pub(crate) fn with_url_opener(url_opener: Arc<dyn UrlOpener>) -> Self {
        Self {
            inner: Arc::new(AccountServiceInner {
                browser_open_operation: Mutex::new(()),
                login_operation: Mutex::new(()),
                state: Mutex::new(AccountServiceState {
                    connection: None,
                    connection_revision: 0,
                    login_attempt: None,
                    refresh_revision: 0,
                    status: AccountStatus::Checking,
                }),
                url_opener,
            }),
        }
    }

    pub(crate) fn connect(&self, connection: Arc<dyn ConnectionControl>) -> AccountStatus {
        let _browser_open = self
            .inner
            .browser_open_operation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.connection_revision = state.connection_revision.wrapping_add(1);
        state.connection = Some(connection);
        state.login_attempt = None;
        state.status = AccountStatus::Checking;
        state.status.clone()
    }

    pub(crate) fn disconnect(&self) -> AccountStatus {
        let _browser_open = self
            .inner
            .browser_open_operation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.connection_revision = state.connection_revision.wrapping_add(1);
        state.connection = None;
        state.login_attempt = None;
        state.status = retryable_account_error();
        state.status.clone()
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
                    "useHostedLoginSuccessPage": true,
                    "appBrand": "chatgpt",
                }),
            )
            .ok()
            .and_then(parse_login_response);
        let (login_id, auth_url) = match response {
            Some(LoginStartResponse::Chatgpt { login_id, auth_url }) => (login_id, auth_url),
            Some(LoginStartResponse::ChatgptDeviceCode { login_id, .. })
                if !login_id.is_empty() =>
            {
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
                return self
                    .set_status_for_connection(connection_revision, login_unavailable_error());
            }
        };
        if login_id.is_empty() {
            return self.set_status_for_connection(connection_revision, login_unavailable_error());
        }
        let attempt = LoginAttempt { login_id };
        let Some(auth_url) = parse_official_auth_url(&auth_url) else {
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
        if !self.install_attempt(
            connection_revision,
            attempt.clone(),
            AccountStatus::BrowserPending,
        ) {
            let status = self.status();
            drop(browser_open);
            return self.cancel_started_attempt(&connection, connection_revision, attempt, status);
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
        Ok((connection, state.connection_revision))
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
            return Err(
                self.set_status_for_connection(connection_revision, login_unavailable_error())
            );
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

    fn install_attempt(
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

    fn cancel_started_attempt(
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

    fn clear_attempt(&self, login_id: &str) {
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
        }
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountReadResponse {
    account: Value,
    requires_openai_auth: bool,
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum AccountPayload {
    Chatgpt {
        email: Value,
        plan_type: String,
    },
    #[serde(other)]
    Unsupported,
}

fn parse_account_status(result: Value) -> AccountStatus {
    let Ok(response) = serde_json::from_value::<AccountReadResponse>(result) else {
        return retryable_account_error();
    };

    if response.account.is_null() {
        return if response.requires_openai_auth {
            AccountStatus::SignedOut
        } else {
            unsupported_account_error()
        };
    }

    let Ok(account) = serde_json::from_value::<AccountPayload>(response.account) else {
        return retryable_account_error();
    };
    match account {
        AccountPayload::Chatgpt { email, plan_type } => {
            let email = match email {
                Value::Null => None,
                Value::String(email) => Some(email),
                _ => return retryable_account_error(),
            };
            AccountStatus::SignedIn { email, plan_type }
        }
        AccountPayload::Unsupported => unsupported_account_error(),
    }
}

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
