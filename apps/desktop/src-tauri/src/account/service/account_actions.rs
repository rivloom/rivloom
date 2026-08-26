use std::sync::PoisonError;

use serde_json::Value;

use super::AccountService;
use crate::account::types::AccountStatus;

const LOGOUT_UNAVAILABLE_MESSAGE: &str = "无法退出 ChatGPT，请重试。";

impl AccountService {
    pub(crate) fn cancel_account_login(&self) -> AccountStatus {
        let _operation = self
            .inner
            .login_operation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Err(status) = self.cancel_active_attempt() {
            return status;
        }
        self.refresh()
    }

    pub(crate) fn logout_account(&self) -> AccountStatus {
        let _operation = self
            .inner
            .login_operation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Err(status) = self.cancel_active_attempt() {
            return status;
        }
        let (connection, connection_revision) = {
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let Some(connection) = state.connection.clone() else {
                return logout_unavailable_error();
            };
            (connection, state.connection_revision)
        };
        let response = connection.request_without_params("account/logout");
        if !self.is_current_connection(connection_revision) {
            return self.status();
        }
        if !matches!(response, Ok(Value::Object(object)) if object.is_empty()) {
            return logout_unavailable_error();
        }
        self.refresh()
    }

    fn is_current_connection(&self, connection_revision: u64) -> bool {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.connection_revision == connection_revision && state.connection.is_some()
    }
}

fn logout_unavailable_error() -> AccountStatus {
    AccountStatus::Error {
        message: LOGOUT_UNAVAILABLE_MESSAGE.to_string(),
        retryable: true,
    }
}
