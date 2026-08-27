use std::sync::Arc;

use tauri::AppHandle;
use tauri::Emitter;
use tauri::Url;
use tauri_plugin_shell::ShellExt;

use super::AccountCommand;
use super::AccountService;
use super::AccountStatus;
use super::login::UrlOpener;
use super::service::AccountStatusObserver;
use crate::app_server::log_diagnostic;

const ACCOUNT_STATUS_CHANGED_EVENT: &str = "account-status-changed";

pub(crate) struct AccountState {
    service: AccountService,
}

impl AccountState {
    pub(crate) fn new(app_handle: AppHandle) -> Self {
        let url_opener = Arc::new(TauriUrlOpener {
            app_handle: app_handle.clone(),
        });
        let observer = Arc::new(AccountStatusEventEmitter { app_handle });
        Self {
            service: AccountService::with_runtime_dependencies(url_opener, observer),
        }
    }

    pub(crate) fn service(&self) -> AccountService {
        self.service.clone()
    }

    pub(crate) fn execute(&self, command: AccountCommand) -> AccountStatus {
        self.service.execute_command(command)
    }
}

struct TauriUrlOpener {
    app_handle: AppHandle,
}

impl UrlOpener for TauriUrlOpener {
    #[allow(deprecated)]
    fn open(&self, url: &Url) -> Result<(), ()> {
        self.app_handle
            .shell()
            .open(url.as_str(), /*with*/ None)
            .map_err(|_| ())
    }
}

struct AccountStatusEventEmitter {
    app_handle: AppHandle,
}

impl AccountStatusObserver for AccountStatusEventEmitter {
    fn on_status(&self, status: &AccountStatus) {
        if let Err(error) = self
            .app_handle
            .emit_to("main", ACCOUNT_STATUS_CHANGED_EVENT, status)
        {
            log_diagnostic("account status event failed", &error.to_string());
        }
    }
}
