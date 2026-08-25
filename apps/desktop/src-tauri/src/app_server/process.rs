use std::sync::Arc;
use std::time::Duration;

use crate::app_server::protocol::initialize_request;
use crate::app_server::protocol::initialized_notification;
use crate::app_server::protocol::parse_initialize_response;
use crate::app_server::transport::ProcessLauncher;
use crate::app_server::transport::ProcessTransport;
use crate::app_server::transport::TransportReadError;
use crate::app_server::transport::log_diagnostic;
use crate::runtime_status::RuntimeStatus;

const STARTUP_ERROR_MESSAGE: &str = "核心服务暂时无法启动。";

pub(super) trait StatusObserver: Send + Sync {
    fn on_status(&self, status: &RuntimeStatus);
}

pub(crate) struct AppServerSupervisor {
    launcher: Box<dyn ProcessLauncher>,
    observer: Arc<dyn StatusObserver>,
    transport: Option<ProcessTransport>,
    status: RuntimeStatus,
    initialization_timeout: Duration,
}

impl AppServerSupervisor {
    pub(super) fn new(
        launcher: Box<dyn ProcessLauncher>,
        observer: Arc<dyn StatusObserver>,
        initialization_timeout: Duration,
    ) -> Self {
        let status = RuntimeStatus::Stopped;
        observer.on_status(&status);

        Self {
            launcher,
            observer,
            transport: None,
            status,
            initialization_timeout,
        }
    }

    pub(crate) fn start(&mut self) -> RuntimeStatus {
        if matches!(
            self.status,
            RuntimeStatus::Starting | RuntimeStatus::Connected { .. }
        ) {
            return self.status.clone();
        }

        self.transition(RuntimeStatus::Starting);
        let mut transport = match self.launcher.launch() {
            Ok(transport) => transport,
            Err(error) => return self.fail("launch failed", &error),
        };

        match self.initialize(&mut transport) {
            Ok(status) => {
                self.transport = Some(transport);
                self.transition(status)
            }
            Err(error) => {
                if let Err(terminate_error) = transport.control().terminate() {
                    log_diagnostic("cleanup failed", &terminate_error);
                }
                self.fail("initialization failed", &error)
            }
        }
    }

    pub(crate) fn retry(&mut self) -> RuntimeStatus {
        if matches!(self.status, RuntimeStatus::Error { .. }) {
            self.start()
        } else {
            self.status.clone()
        }
    }

    pub(crate) fn shutdown(&mut self) -> RuntimeStatus {
        if matches!(self.status, RuntimeStatus::Stopped) {
            return self.status.clone();
        }

        if let Some(transport) = self.transport.take()
            && let Err(error) = transport.control().terminate()
        {
            log_diagnostic("shutdown failed", &error);
        }
        self.transition(RuntimeStatus::Stopped)
    }

    fn initialize(&self, transport: &mut ProcessTransport) -> Result<RuntimeStatus, String> {
        let control = transport.control();
        let request = initialize_request().map_err(|error| error.to_string())?;
        control.write(&request)?;

        let response = transport
            .receive_line(self.initialization_timeout)
            .map_err(initialization_error_detail)?;
        let status = parse_initialize_response(&response).map_err(|error| error.to_string())?;

        let notification = initialized_notification().map_err(|error| error.to_string())?;
        control.write(&notification)?;
        Ok(status)
    }

    fn fail(&mut self, context: &str, detail: &str) -> RuntimeStatus {
        log_diagnostic(context, detail);
        self.transition(RuntimeStatus::Error {
            message: STARTUP_ERROR_MESSAGE.to_string(),
            retryable: true,
        })
    }

    fn transition(&mut self, status: RuntimeStatus) -> RuntimeStatus {
        self.status = status;
        self.observer.on_status(&self.status);
        self.status.clone()
    }
}

impl Drop for AppServerSupervisor {
    fn drop(&mut self) {
        if let Some(transport) = self.transport.take()
            && let Err(error) = transport.control().terminate()
        {
            log_diagnostic("drop cleanup failed", &error);
        }
    }
}

fn initialization_error_detail(error: TransportReadError) -> String {
    match error {
        TransportReadError::Timeout => "initialization timed out".to_string(),
        TransportReadError::InvalidMessage(error) => error.to_string(),
        TransportReadError::Transport(message) => message,
        TransportReadError::Terminated(code) => {
            format!("App Server exited before initialization completed with code {code:?}")
        }
        TransportReadError::Closed => {
            "App Server event channel closed before initialization completed".to_string()
        }
    }
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
