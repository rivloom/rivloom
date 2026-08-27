use std::sync::PoisonError;

use super::AccountService;
use super::retryable_account_error;
use crate::account::types::AccountStatus;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AccountCommand {
    GetStatus,
    StartChatgptLogin,
    StartDeviceCodeLogin,
    CancelLogin,
    Logout,
    OpenDeviceVerification,
}

impl AccountService {
    pub(crate) fn execute_command(&self, command: AccountCommand) -> AccountStatus {
        let connected = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .connection
            .is_some();
        let status = if connected {
            match command {
                AccountCommand::GetStatus => self.refresh(),
                AccountCommand::StartChatgptLogin => self.start_browser_login(),
                AccountCommand::StartDeviceCodeLogin => self.start_device_code_login(),
                AccountCommand::CancelLogin => self.cancel_account_login(),
                AccountCommand::Logout => self.logout_account(),
                AccountCommand::OpenDeviceVerification => self.open_device_verification(),
            }
        } else {
            retryable_account_error()
        };
        self.publish_status(&status);
        status
    }
}
