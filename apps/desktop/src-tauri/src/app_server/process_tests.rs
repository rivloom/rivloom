use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::json;

use super::AppServerSupervisor;
use super::StatusObserver;
use crate::app_server::protocol::initialize_request;
use crate::app_server::protocol::initialized_notification;
use crate::app_server::transport::ProcessControl;
use crate::app_server::transport::ProcessLauncher;
use crate::app_server::transport::ProcessTransport;
use crate::app_server::transport::TransportEvent;
use crate::runtime_status::RuntimeStatus;

const STARTUP_ERROR_MESSAGE: &str = "核心服务暂时无法启动。";

#[test]
fn successful_start_transitions_from_stopped_through_starting_to_connected() {
    let observer = Arc::new(RecordingObserver::default());
    let mut supervisor = supervisor_with(
        FakeLauncher::succeeding(success_response()),
        observer.clone(),
    );

    assert_eq!(supervisor.start(), connected_status());
    assert_eq!(
        observer.statuses(),
        vec![
            RuntimeStatus::Stopped,
            RuntimeStatus::Starting,
            connected_status(),
        ]
    );
}

#[test]
fn launch_failure_transitions_from_stopped_through_starting_to_error() {
    let observer = Arc::new(RecordingObserver::default());
    let mut supervisor = supervisor_with(
        FakeLauncher::failing("sidecar could not start"),
        observer.clone(),
    );

    assert_eq!(supervisor.start(), startup_error_status());
    assert_eq!(
        observer.statuses(),
        vec![
            RuntimeStatus::Stopped,
            RuntimeStatus::Starting,
            startup_error_status(),
        ]
    );
}

#[test]
fn shutdown_terminates_a_connected_process_and_transitions_to_stopped() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, handle) = FakeProcess::with_response(success_response());
    let mut supervisor =
        supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer.clone());

    assert_eq!(supervisor.start(), connected_status());
    assert_eq!(supervisor.shutdown(), RuntimeStatus::Stopped);
    assert_eq!(supervisor.shutdown(), RuntimeStatus::Stopped);
    assert!(handle.terminated());
    assert_eq!(
        observer.statuses(),
        vec![
            RuntimeStatus::Stopped,
            RuntimeStatus::Starting,
            connected_status(),
            RuntimeStatus::Stopped,
        ]
    );
}

#[test]
fn manual_retry_transitions_from_error_back_through_starting_to_connected() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, _handle) = FakeProcess::with_response(success_response());
    let launcher =
        FakeLauncher::with_attempts([Err("first launch failed".to_string()), Ok(process)]);
    let mut supervisor = supervisor_with(launcher, observer.clone());

    assert_eq!(supervisor.start(), startup_error_status());
    assert_eq!(supervisor.retry(), connected_status());
    assert_eq!(
        observer.statuses(),
        vec![
            RuntimeStatus::Stopped,
            RuntimeStatus::Starting,
            startup_error_status(),
            RuntimeStatus::Starting,
            connected_status(),
        ]
    );
}

#[test]
fn successful_handshake_writes_initialize_then_initialized() {
    let (process, handle) = FakeProcess::with_response(success_response());
    let mut supervisor = supervisor_with(
        FakeLauncher::with_attempts([Ok(process)]),
        Arc::new(RecordingObserver::default()),
    );

    assert_eq!(supervisor.start(), connected_status());
    assert_eq!(
        handle.writes(),
        vec![
            initialize_request().unwrap(),
            initialized_notification().unwrap(),
        ]
    );
}

#[test]
fn initialization_timeout_transitions_to_error_and_terminates_the_process() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, handle) = FakeProcess::without_events();
    let mut supervisor =
        supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer.clone());

    assert_eq!(supervisor.start(), startup_error_status());
    assert!(handle.terminated());
    assert_eq!(observer.statuses().last(), Some(&startup_error_status()));
}

fn supervisor_with(
    launcher: FakeLauncher,
    observer: Arc<RecordingObserver>,
) -> AppServerSupervisor {
    AppServerSupervisor::new(
        Box::new(launcher),
        observer,
        Duration::from_millis(/*millis*/ 5),
    )
}

fn success_response() -> String {
    json!({
        "id": 0,
        "result": {
            "userAgent": "codex-app-server/1.2.3",
            "codexHome": r"C:\Users\demo\Rivloom\codex-home",
            "platformFamily": "windows",
            "platformOs": "windows",
        },
    })
    .to_string()
}

fn connected_status() -> RuntimeStatus {
    RuntimeStatus::Connected {
        app_version: "0.1.0-alpha.0".to_string(),
        app_server_user_agent: "codex-app-server/1.2.3".to_string(),
        platform: "windows/windows".to_string(),
        codex_home: r"C:\Users\demo\Rivloom\codex-home".to_string(),
    }
}

fn startup_error_status() -> RuntimeStatus {
    RuntimeStatus::Error {
        message: STARTUP_ERROR_MESSAGE.to_string(),
        retryable: true,
    }
}

#[derive(Default)]
struct RecordingObserver {
    statuses: Mutex<Vec<RuntimeStatus>>,
}

impl RecordingObserver {
    fn statuses(&self) -> Vec<RuntimeStatus> {
        self.statuses.lock().unwrap().clone()
    }
}

impl StatusObserver for RecordingObserver {
    fn on_status(&self, status: &RuntimeStatus) {
        self.statuses.lock().unwrap().push(status.clone());
    }
}

struct FakeLauncher {
    attempts: VecDeque<Result<ProcessTransport, String>>,
}

impl FakeLauncher {
    fn succeeding(response: String) -> Self {
        let (process, _handle) = FakeProcess::with_response(response);
        Self::with_attempts([Ok(process)])
    }

    fn failing(message: &str) -> Self {
        Self::with_attempts([Err(message.to_string())])
    }

    fn with_attempts(attempts: impl IntoIterator<Item = Result<ProcessTransport, String>>) -> Self {
        Self {
            attempts: attempts.into_iter().collect(),
        }
    }
}

impl ProcessLauncher for FakeLauncher {
    fn launch(&mut self) -> Result<ProcessTransport, String> {
        self.attempts
            .pop_front()
            .unwrap_or_else(|| Err("unexpected launch attempt".to_string()))
    }
}

struct FakeProcess;

impl FakeProcess {
    fn with_response(response: String) -> (ProcessTransport, FakeProcessHandle) {
        let (process, handle) = Self::without_events();
        handle
            .send_event(TransportEvent::Stdout(format!("{response}\n").into_bytes()))
            .unwrap();
        (process, handle)
    }

    fn without_events() -> (ProcessTransport, FakeProcessHandle) {
        let (events, receiver) = mpsc::channel();
        let control = Arc::new(FakeProcessControl {
            writes: Mutex::new(Vec::new()),
            terminated: AtomicBool::new(false),
            events,
        });
        let handle = FakeProcessHandle {
            control: control.clone(),
        };
        (ProcessTransport::new(control, receiver), handle)
    }
}

#[derive(Clone)]
struct FakeProcessHandle {
    control: Arc<FakeProcessControl>,
}

impl FakeProcessHandle {
    fn writes(&self) -> Vec<String> {
        self.control.writes.lock().unwrap().clone()
    }

    fn terminated(&self) -> bool {
        self.control.terminated.load(Ordering::SeqCst)
    }

    fn send_event(&self, event: TransportEvent) -> Result<(), String> {
        self.control
            .events
            .send(event)
            .map_err(|error| error.to_string())
    }
}

struct FakeProcessControl {
    writes: Mutex<Vec<String>>,
    terminated: AtomicBool,
    events: mpsc::Sender<TransportEvent>,
}

impl ProcessControl for FakeProcessControl {
    fn write(&self, message: &str) -> Result<(), String> {
        self.writes.lock().unwrap().push(message.to_string());
        Ok(())
    }

    fn terminate(&self) -> Result<(), String> {
        self.terminated.store(true, Ordering::SeqCst);
        Ok(())
    }
}
