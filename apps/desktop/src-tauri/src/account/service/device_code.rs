use std::sync::PoisonError;

use serde_json::json;

use super::AccountService;
use super::LoginAttempt;
use super::LoginAttemptKind;
use super::login_unavailable_error;
use crate::account::login::LoginStartResponse;
use crate::account::login::parse_login_response;
use crate::account::login::parse_official_auth_url;
use crate::account::types::AccountStatus;

const DEVICE_VERIFICATION_OPEN_MESSAGE: &str = "无法打开设备码验证页面，请手动打开该地址后重试。";

impl AccountService {
    pub(crate) fn start_device_code_login(&self) -> AccountStatus {
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
                json!({ "type": "chatgptDeviceCode" }),
            )
            .ok()
            .and_then(parse_login_response);
        let (login_id, verification_url, user_code) = match response {
            Some(LoginStartResponse::ChatgptDeviceCode {
                login_id,
                verification_url,
                user_code,
            }) => (login_id, verification_url, user_code),
            Some(LoginStartResponse::Chatgpt { login_id, .. }) if !login_id.is_empty() => {
                return self.cancel_started_attempt(
                    &connection,
                    connection_revision,
                    LoginAttempt {
                        login_id,
                        kind: LoginAttemptKind::CleanupOnly,
                    },
                    login_unavailable_error(),
                );
            }
            Some(LoginStartResponse::Chatgpt { .. })
            | Some(LoginStartResponse::Unsupported)
            | None => {
                return self
                    .set_status_for_connection(connection_revision, login_unavailable_error());
            }
        };
        if login_id.is_empty() {
            return self.set_status_for_connection(connection_revision, login_unavailable_error());
        }
        let Some(verification_url) = parse_official_auth_url(&verification_url) else {
            return self.cancel_started_attempt(
                &connection,
                connection_revision,
                LoginAttempt {
                    login_id,
                    kind: LoginAttemptKind::CleanupOnly,
                },
                login_unavailable_error(),
            );
        };
        if user_code.trim().is_empty() {
            return self.cancel_started_attempt(
                &connection,
                connection_revision,
                LoginAttempt {
                    login_id,
                    kind: LoginAttemptKind::CleanupOnly,
                },
                login_unavailable_error(),
            );
        }
        let status = AccountStatus::DevicePending {
            verification_url: verification_url.as_str().to_string(),
            user_code: user_code.clone(),
        };
        let attempt = LoginAttempt {
            login_id,
            kind: LoginAttemptKind::DeviceCode {
                verification_url,
                user_code,
            },
        };
        if self.install_attempt(connection_revision, attempt.clone(), status) {
            return self.status();
        }
        let status = self.status();
        self.cancel_started_attempt(&connection, connection_revision, attempt, status)
    }

    pub(crate) fn open_device_verification(&self) -> AccountStatus {
        let _operation = self
            .inner
            .login_operation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let _browser_open = self
            .inner
            .browser_open_operation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (connection_revision, login_id, verification_url, user_code) = {
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let Some(LoginAttempt {
                login_id,
                kind:
                    LoginAttemptKind::DeviceCode {
                        verification_url,
                        user_code,
                    },
            }) = &state.login_attempt
            else {
                return state.status.clone();
            };
            (
                state.connection_revision,
                login_id.clone(),
                verification_url.clone(),
                user_code.clone(),
            )
        };
        let next_status = if self.inner.url_opener.open(&verification_url).is_ok() {
            AccountStatus::DevicePending {
                verification_url: verification_url.as_str().to_string(),
                user_code,
            }
        } else {
            device_verification_open_error()
        };
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
            state.status = next_status;
        }
        state.status.clone()
    }
}

fn device_verification_open_error() -> AccountStatus {
    AccountStatus::Error {
        message: DEVICE_VERIFICATION_OPEN_MESSAGE.to_string(),
        retryable: true,
    }
}
