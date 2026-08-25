use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

use crate::account::types::AccountStatus;
use crate::app_server::ConnectionControl;
use crate::app_server::ConnectionError;

const ACCOUNT_UNAVAILABLE_MESSAGE: &str = "账号状态暂时不可用。";
const UNSUPPORTED_ACCOUNT_MESSAGE: &str = "当前核心服务配置不支持 ChatGPT 账号登录。";

#[derive(Clone)]
pub(crate) struct AccountService {
    inner: Arc<AccountServiceInner>,
}

struct AccountServiceInner {
    state: Mutex<AccountServiceState>,
}

struct AccountServiceState {
    connection: Option<Arc<dyn ConnectionControl>>,
    connection_revision: u64,
    status: AccountStatus,
}

impl AccountService {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(AccountServiceInner {
                state: Mutex::new(AccountServiceState {
                    connection: None,
                    connection_revision: 0,
                    status: AccountStatus::Checking,
                }),
            }),
        }
    }

    pub(crate) fn connect(&self, connection: Arc<dyn ConnectionControl>) -> AccountStatus {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.connection_revision = state.connection_revision.wrapping_add(1);
        state.connection = Some(connection);
        state.status = AccountStatus::Checking;
        state.status.clone()
    }

    pub(crate) fn disconnect(&self) -> AccountStatus {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.connection_revision = state.connection_revision.wrapping_add(1);
        state.connection = None;
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
        let (connection, connection_revision) = {
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            (state.connection.clone(), state.connection_revision)
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
        if state.connection_revision == connection_revision {
            state.status = next_status;
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
#[serde(rename_all = "camelCase")]
struct AccountPayload {
    #[serde(rename = "type")]
    account_type: String,
    email: Option<String>,
    plan_type: Option<String>,
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
    match account.account_type.as_str() {
        "chatgpt" => match account.plan_type {
            Some(plan_type) => AccountStatus::SignedIn {
                email: account.email,
                plan_type,
            },
            None => retryable_account_error(),
        },
        _ => unsupported_account_error(),
    }
}

fn retryable_account_error() -> AccountStatus {
    AccountStatus::Error {
        message: ACCOUNT_UNAVAILABLE_MESSAGE.to_string(),
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
