use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use tauri::AppHandle;
use tauri::Emitter;

use crate::app_server::process::AppServerSupervisor;
use crate::app_server::process::StatusObserver;
use crate::app_server::transport::TauriProcessLauncher;
use crate::app_server::transport::log_diagnostic;
use crate::runtime_status::RuntimeStatus;

const INITIALIZATION_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(/*secs*/ 10);
const RUNTIME_STATUS_CHANGED_EVENT: &str = "runtime-status-changed";

pub(crate) struct AppServerState {
    supervisor: Mutex<AppServerSupervisor>,
    status: Arc<RuntimeStatusStore>,
}

impl AppServerState {
    pub(crate) fn new(app_handle: AppHandle, codex_home: PathBuf) -> Self {
        let status = Arc::new(RuntimeStatusStore::new(app_handle.clone()));
        let observer: Arc<dyn StatusObserver> = status.clone();
        let launcher = Box::new(TauriProcessLauncher::new(app_handle, codex_home));

        Self {
            supervisor: Mutex::new(AppServerSupervisor::new(
                launcher,
                observer,
                INITIALIZATION_TIMEOUT,
            )),
            status,
        }
    }

    pub(crate) fn current_status(&self) -> RuntimeStatus {
        self.status.current()
    }

    pub(crate) fn start(&self) -> RuntimeStatus {
        self.supervisor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .start()
    }

    pub(crate) fn retry(&self) -> RuntimeStatus {
        self.supervisor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retry()
    }

    pub(crate) fn shutdown(&self) -> RuntimeStatus {
        self.supervisor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .shutdown()
    }
}

struct RuntimeStatusStore {
    app_handle: AppHandle,
    current: Mutex<RuntimeStatus>,
}

impl RuntimeStatusStore {
    fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            current: Mutex::new(RuntimeStatus::Stopped),
        }
    }

    fn current(&self) -> RuntimeStatus {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl StatusObserver for RuntimeStatusStore {
    fn on_status(&self, status: &RuntimeStatus) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = status.clone();

        if let Err(error) = self
            .app_handle
            .emit_to("main", RUNTIME_STATUS_CHANGED_EVENT, status)
        {
            log_diagnostic("status event failed", &error.to_string());
        }

        #[cfg(debug_assertions)]
        log_runtime_status(status);
    }
}

#[cfg(debug_assertions)]
fn log_runtime_status(status: &RuntimeStatus) {
    match status {
        RuntimeStatus::Starting => log_diagnostic("status", "starting"),
        RuntimeStatus::Connected {
            app_server_user_agent,
            platform,
            codex_home,
            ..
        } => log_diagnostic(
            "status",
            &format!(
                "connected; userAgent={app_server_user_agent}; platform={platform}; codexHome={codex_home}"
            ),
        ),
        RuntimeStatus::Error { .. } => log_diagnostic("status", "error"),
        RuntimeStatus::Stopped => log_diagnostic("status", "stopped"),
    }
}
