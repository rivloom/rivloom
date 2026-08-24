use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;
use std::time::Instant;

use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_shell::process::CommandEvent;

use crate::app_server::protocol::initialize_request;
use crate::app_server::protocol::initialized_notification;
use crate::app_server::protocol::parse_initialize_response;
use crate::runtime_status::RuntimeStatus;

const STARTUP_ERROR_MESSAGE: &str = "核心服务暂时无法启动。";
const MAX_DIAGNOSTIC_CHARS: usize = 512;
const SIDECAR_NAME: &str = "codex-app-server";
const INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) trait ProcessLauncher: Send {
    fn launch(&mut self) -> Result<Box<dyn ProcessChild>, String>;
}

pub(super) trait ProcessChild: Send {
    fn write(&mut self, message: &str) -> Result<(), String>;
    fn receive_line(&mut self, timeout: Duration) -> Result<String, ChildReadError>;
    fn terminate(&mut self) -> Result<(), String>;
}

#[derive(Debug)]
pub(super) enum ChildReadError {
    Timeout,
    Transport(String),
}

pub(super) trait StatusObserver: Send + Sync {
    fn on_status(&self, status: &RuntimeStatus);
}

pub(crate) struct AppServerSupervisor {
    launcher: Box<dyn ProcessLauncher>,
    observer: Arc<dyn StatusObserver>,
    child: Option<Box<dyn ProcessChild>>,
    status: RuntimeStatus,
    initialization_timeout: Duration,
}

pub(crate) struct AppServerState {
    supervisor: Mutex<AppServerSupervisor>,
    status: Arc<RuntimeStatusStore>,
}

impl AppServerState {
    pub(crate) fn new(app_handle: AppHandle, codex_home: PathBuf) -> Self {
        let status = Arc::new(RuntimeStatusStore::new());
        let observer: Arc<dyn StatusObserver> = status.clone();
        let launcher = Box::new(TauriProcessLauncher {
            app_handle,
            codex_home,
        });

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
            child: None,
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
        let mut child = match self.launcher.launch() {
            Ok(child) => child,
            Err(error) => return self.fail("launch failed", &error),
        };

        let result = self.initialize(&mut *child);
        match result {
            Ok(status) => {
                self.child = Some(child);
                self.transition(status)
            }
            Err(error) => {
                if let Err(terminate_error) = child.terminate() {
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

        if let Some(mut child) = self.child.take()
            && let Err(error) = child.terminate()
        {
            log_diagnostic("shutdown failed", &error);
        }
        self.transition(RuntimeStatus::Stopped)
    }

    fn initialize(&self, child: &mut dyn ProcessChild) -> Result<RuntimeStatus, String> {
        let request = initialize_request().map_err(|error| error.to_string())?;
        child.write(&request)?;

        let response =
            child
                .receive_line(self.initialization_timeout)
                .map_err(|error| match error {
                    ChildReadError::Timeout => "initialization timed out".to_string(),
                    ChildReadError::Transport(message) => message,
                })?;
        let status = parse_initialize_response(&response).map_err(|error| error.to_string())?;

        let notification = initialized_notification().map_err(|error| error.to_string())?;
        child.write(&notification)?;
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
        if let Some(mut child) = self.child.take()
            && let Err(error) = child.terminate()
        {
            log_diagnostic("drop cleanup failed", &error);
        }
    }
}

fn log_diagnostic(context: &str, detail: &str) {
    let detail = detail
        .chars()
        .take(MAX_DIAGNOSTIC_CHARS)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    eprintln!("Rivloom App Server {context}: {detail}");
}

struct RuntimeStatusStore {
    current: Mutex<RuntimeStatus>,
}

impl RuntimeStatusStore {
    fn new() -> Self {
        Self {
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

struct TauriProcessLauncher {
    app_handle: AppHandle,
    codex_home: PathBuf,
}

impl ProcessLauncher for TauriProcessLauncher {
    fn launch(&mut self) -> Result<Box<dyn ProcessChild>, String> {
        std::fs::create_dir_all(&self.codex_home)
            .map_err(|error| format!("failed to create Codex home: {error}"))?;

        let command = self
            .app_handle
            .shell()
            .sidecar(SIDECAR_NAME)
            .map_err(|error| format!("failed to resolve bundled sidecar: {error}"))?
            .env("CODEX_HOME", &self.codex_home);
        let (mut events, child) = command
            .spawn()
            .map_err(|error| format!("failed to spawn bundled sidecar: {error}"))?;
        let (event_sender, event_receiver) = mpsc::channel();

        let _event_forwarder = tauri::async_runtime::spawn(async move {
            while let Some(event) = events.recv().await {
                let transport_event = match event {
                    CommandEvent::Stdout(bytes) => TransportEvent::Stdout(bytes),
                    CommandEvent::Stderr(bytes) => {
                        log_diagnostic("stderr", &String::from_utf8_lossy(&bytes));
                        continue;
                    }
                    CommandEvent::Error(message) => TransportEvent::Error(message),
                    CommandEvent::Terminated(payload) => TransportEvent::Terminated(payload.code),
                    _ => continue,
                };

                if event_sender.send(transport_event).is_err() {
                    break;
                }
            }
        });

        Ok(Box::new(TauriProcessChild {
            child: Some(child),
            events: event_receiver,
        }))
    }
}

enum TransportEvent {
    Stdout(Vec<u8>),
    Error(String),
    Terminated(Option<i32>),
}

struct TauriProcessChild {
    child: Option<CommandChild>,
    events: mpsc::Receiver<TransportEvent>,
}

impl ProcessChild for TauriProcessChild {
    fn write(&mut self, message: &str) -> Result<(), String> {
        self.child
            .as_mut()
            .ok_or_else(|| "App Server process is no longer running".to_string())?
            .write(message.as_bytes())
            .map_err(|error| format!("failed to write to App Server stdin: {error}"))
    }

    fn receive_line(&mut self, timeout: Duration) -> Result<String, ChildReadError> {
        let deadline = Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ChildReadError::Timeout);
            }

            match self.events.recv_timeout(remaining) {
                Ok(TransportEvent::Stdout(bytes)) => {
                    return String::from_utf8(bytes).map_err(|_| {
                        ChildReadError::Transport(
                            "App Server stdout was not valid UTF-8".to_string(),
                        )
                    });
                }
                Ok(TransportEvent::Error(message)) => {
                    return Err(ChildReadError::Transport(message));
                }
                Ok(TransportEvent::Terminated(code)) => {
                    return Err(ChildReadError::Transport(format!(
                        "App Server exited before initialization completed with code {code:?}"
                    )));
                }
                Err(RecvTimeoutError::Timeout) => return Err(ChildReadError::Timeout),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(ChildReadError::Transport(
                        "App Server event channel closed before initialization completed"
                            .to_string(),
                    ));
                }
            }
        }
    }

    fn terminate(&mut self) -> Result<(), String> {
        if let Some(child) = self.child.take() {
            child
                .kill()
                .map_err(|error| format!("failed to terminate App Server: {error}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
