use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::app_server::connection::AppServerConnection;
use crate::app_server::protocol::initialize_request;
use crate::app_server::protocol::initialized_notification;
use crate::app_server::protocol::parse_initialize_response;
use crate::app_server::transport::ProcessControl;
use crate::app_server::transport::ProcessLauncher;
use crate::app_server::transport::ProcessTransport;
use crate::app_server::transport::TransportReadError;
use crate::app_server::transport::log_diagnostic;
use crate::app_server::wire::parse_inbound_message;
use crate::runtime_status::RuntimeStatus;

const STARTUP_ERROR_MESSAGE: &str = "核心服务暂时无法启动。";
const READER_POLL_INTERVAL: Duration = Duration::from_millis(/*millis*/ 25);

pub(super) trait StatusObserver: Send + Sync {
    fn on_status(&self, status: &RuntimeStatus);
}

pub(crate) struct AppServerSupervisor {
    launcher: Box<dyn ProcessLauncher>,
    observer: Arc<dyn StatusObserver>,
    lifecycle: Arc<Mutex<LifecycleState>>,
    reader: Option<ReaderTask>,
    initialization_timeout: Duration,
}

struct LifecycleState {
    status: RuntimeStatus,
    next_generation: u64,
    active: Option<ActiveProcess>,
}

struct ActiveProcess {
    generation: u64,
    connection: AppServerConnection,
    control: Arc<dyn ProcessControl>,
    stop: Arc<AtomicBool>,
}

struct ReaderTask {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
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
            lifecycle: Arc::new(Mutex::new(LifecycleState {
                status,
                next_generation: 1,
                active: None,
            })),
            reader: None,
            initialization_timeout,
        }
    }

    pub(crate) fn start(&mut self) -> RuntimeStatus {
        let status = self.current_status();
        if matches!(
            status,
            RuntimeStatus::Starting | RuntimeStatus::Connected { .. }
        ) {
            return status;
        }

        self.join_reader();
        self.transition(RuntimeStatus::Starting);
        let mut transport = match self.launcher.launch() {
            Ok(transport) => transport,
            Err(error) => return self.fail("launch failed", &error),
        };

        let status = match self.initialize(&mut transport) {
            Ok(status) => status,
            Err(error) => {
                if let Err(terminate_error) = transport.control().terminate() {
                    log_diagnostic("cleanup failed", &terminate_error);
                }
                return self.fail("initialization failed", &error);
            }
        };

        self.activate_connection(transport, status)
    }

    pub(crate) fn retry(&mut self) -> RuntimeStatus {
        if matches!(self.current_status(), RuntimeStatus::Error { .. }) {
            self.start()
        } else {
            self.current_status()
        }
    }

    pub(crate) fn shutdown(&mut self) -> RuntimeStatus {
        if matches!(self.current_status(), RuntimeStatus::Stopped) {
            return RuntimeStatus::Stopped;
        }

        self.cleanup_active("shutdown failed");
        self.join_reader();
        self.transition(RuntimeStatus::Stopped)
    }

    pub(super) fn connection(&self) -> Option<AppServerConnection> {
        self.lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .active
            .as_ref()
            .map(|active| active.connection.clone())
    }

    fn initialize(&self, transport: &mut ProcessTransport) -> Result<RuntimeStatus, String> {
        let control = transport.control();
        let request = initialize_request().map_err(|error| error.to_string())?;
        control.write(&request)?;

        let response = transport
            .receive_line(self.initialization_timeout)
            .map_err(transport_error_detail)?;
        let status = parse_initialize_response(&response).map_err(|error| error.to_string())?;

        let notification = initialized_notification().map_err(|error| error.to_string())?;
        control.write(&notification)?;
        Ok(status)
    }

    fn activate_connection(
        &mut self,
        transport: ProcessTransport,
        status: RuntimeStatus,
    ) -> RuntimeStatus {
        let control = transport.control();
        let writer = control.clone();
        let connection = AppServerConnection::new(move |message| writer.write(message));
        let stop = Arc::new(AtomicBool::new(false));
        let generation = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let generation = lifecycle.next_generation;
            lifecycle.next_generation = lifecycle.next_generation.wrapping_add(1);
            lifecycle.status = status.clone();
            lifecycle.active = Some(ActiveProcess {
                generation,
                connection: connection.clone(),
                control,
                stop: stop.clone(),
            });
            generation
        };
        self.observer.on_status(&status);

        let lifecycle = self.lifecycle.clone();
        let observer = self.observer.clone();
        let reader_stop = stop.clone();
        let reader = thread::Builder::new()
            .name("rivloom-app-server-reader".to_string())
            .spawn(move || {
                read_messages(
                    transport,
                    connection,
                    lifecycle,
                    observer,
                    generation,
                    reader_stop,
                );
            });

        match reader {
            Ok(handle) => {
                self.reader = Some(ReaderTask { stop, handle });
                status
            }
            Err(error) => {
                fail_active_connection(
                    &self.lifecycle,
                    &self.observer,
                    generation,
                    "reader start failed",
                    &error.to_string(),
                    true,
                );
                self.current_status()
            }
        }
    }

    fn current_status(&self) -> RuntimeStatus {
        self.lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .status
            .clone()
    }

    fn fail(&self, context: &str, detail: &str) -> RuntimeStatus {
        log_diagnostic(context, detail);
        self.transition(startup_error_status())
    }

    fn transition(&self, status: RuntimeStatus) -> RuntimeStatus {
        self.lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .status = status.clone();
        self.observer.on_status(&status);
        status
    }

    fn cleanup_active(&self, context: &str) {
        let active = self
            .lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .active
            .take();
        if let Some(active) = active {
            active.stop.store(true, Ordering::Release);
            active.connection.disconnect();
            if let Err(error) = active.control.terminate() {
                log_diagnostic(context, &error);
            }
        }
    }

    fn join_reader(&mut self) {
        if let Some(reader) = self.reader.take() {
            reader.stop.store(true, Ordering::Release);
            if reader.handle.join().is_err() {
                log_diagnostic("reader join failed", "reader thread panicked");
            }
        }
    }
}

impl Drop for AppServerSupervisor {
    fn drop(&mut self) {
        self.cleanup_active("drop cleanup failed");
        self.join_reader();
    }
}

fn read_messages(
    mut transport: ProcessTransport,
    connection: AppServerConnection,
    lifecycle: Arc<Mutex<LifecycleState>>,
    observer: Arc<dyn StatusObserver>,
    generation: u64,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Acquire) {
        let line = match transport.receive_line(READER_POLL_INTERVAL) {
            Ok(line) => line,
            Err(TransportReadError::Timeout) => continue,
            Err(error) => {
                let terminate = !matches!(error, TransportReadError::Terminated(_));
                let detail = transport_error_detail(error);
                fail_active_connection(
                    &lifecycle,
                    &observer,
                    generation,
                    "connection lost",
                    &detail,
                    terminate,
                );
                return;
            }
        };

        let message = match parse_inbound_message(&line) {
            Ok(message) => message,
            Err(error) => {
                fail_active_connection(
                    &lifecycle,
                    &observer,
                    generation,
                    "protocol read failed",
                    &error.to_string(),
                    true,
                );
                return;
            }
        };
        if let Err(error) = connection.handle_inbound(message) {
            fail_active_connection(
                &lifecycle,
                &observer,
                generation,
                "protocol routing failed",
                &error.to_string(),
                true,
            );
            return;
        }
    }

    connection.disconnect();
}

fn fail_active_connection(
    lifecycle: &Mutex<LifecycleState>,
    observer: &Arc<dyn StatusObserver>,
    generation: u64,
    context: &str,
    detail: &str,
    terminate: bool,
) {
    let status = startup_error_status();
    let active = {
        let mut lifecycle = lifecycle.lock().unwrap_or_else(PoisonError::into_inner);
        if lifecycle
            .active
            .as_ref()
            .is_none_or(|active| active.generation != generation)
        {
            return;
        }
        let active = lifecycle.active.take();
        lifecycle.status = status.clone();
        active
    };

    if let Some(active) = active {
        active.stop.store(true, Ordering::Release);
        active.connection.disconnect();
        if terminate && let Err(error) = active.control.terminate() {
            log_diagnostic("connection cleanup failed", &error);
        }
    }
    log_diagnostic(context, detail);
    observer.on_status(&status);
}

fn transport_error_detail(error: TransportReadError) -> String {
    match error {
        TransportReadError::Timeout => "App Server response timed out".to_string(),
        TransportReadError::InvalidMessage(error) => error.to_string(),
        TransportReadError::Transport(message) => message,
        TransportReadError::Terminated(code) => {
            format!("App Server process terminated with code {code:?}")
        }
        TransportReadError::Closed => "App Server event channel closed".to_string(),
    }
}

fn startup_error_status() -> RuntimeStatus {
    RuntimeStatus::Error {
        message: STARTUP_ERROR_MESSAGE.to_string(),
        retryable: true,
    }
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
