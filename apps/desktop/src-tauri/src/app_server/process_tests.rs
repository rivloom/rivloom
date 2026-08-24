use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::AppServerSupervisor;
use super::ChildReadError;
use super::ProcessChild;
use super::ProcessLauncher;
use super::StatusObserver;
use crate::app_server::protocol::initialize_request;
use crate::app_server::protocol::initialized_notification;
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
    let expected_error = startup_error_status();

    assert_eq!(supervisor.start(), expected_error);
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
fn shutdown_terminates_a_connected_child_and_transitions_to_stopped() {
    let observer = Arc::new(RecordingObserver::default());
    let terminated = Arc::new(AtomicBool::new(false));
    let launcher = FakeLauncher::with_attempts([Ok(FakeChild::new(
        [Ok(success_response())],
        Arc::new(Mutex::new(Vec::new())),
        terminated.clone(),
    ))]);
    let mut supervisor = supervisor_with(launcher, observer.clone());

    assert_eq!(supervisor.start(), connected_status());
    assert_eq!(supervisor.shutdown(), RuntimeStatus::Stopped);
    assert_eq!(supervisor.shutdown(), RuntimeStatus::Stopped);
    assert!(terminated.load(Ordering::SeqCst));
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
    let launcher = FakeLauncher::with_attempts([
        Err("first launch failed".to_string()),
        Ok(FakeChild::succeeding(success_response())),
    ]);
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
    let writes = Arc::new(Mutex::new(Vec::new()));
    let launcher = FakeLauncher::with_attempts([Ok(FakeChild::new(
        [Ok(success_response())],
        writes.clone(),
        Arc::new(AtomicBool::new(false)),
    ))]);
    let mut supervisor = supervisor_with(launcher, Arc::new(RecordingObserver::default()));

    assert_eq!(supervisor.start(), connected_status());
    assert_eq!(
        *writes.lock().unwrap(),
        vec![
            initialize_request().unwrap(),
            initialized_notification().unwrap(),
        ]
    );
}

#[test]
fn initialization_timeout_transitions_to_error_and_terminates_the_child() {
    let observer = Arc::new(RecordingObserver::default());
    let terminated = Arc::new(AtomicBool::new(false));
    let launcher = FakeLauncher::with_attempts([Ok(FakeChild::new(
        [Err(ChildReadError::Timeout)],
        Arc::new(Mutex::new(Vec::new())),
        terminated.clone(),
    ))]);
    let mut supervisor = supervisor_with(launcher, observer.clone());

    assert_eq!(supervisor.start(), startup_error_status());
    assert!(terminated.load(Ordering::SeqCst));
    assert_eq!(observer.statuses().last(), Some(&startup_error_status()));
}

fn supervisor_with(
    launcher: FakeLauncher,
    observer: Arc<RecordingObserver>,
) -> AppServerSupervisor {
    AppServerSupervisor::new(Box::new(launcher), observer, Duration::from_millis(5))
}

fn success_response() -> String {
    r#"{
        "id": 0,
        "result": {
            "userAgent": "codex-app-server/1.2.3",
            "codexHome": "C:\\Users\\demo\\Rivloom\\codex-home",
            "platformFamily": "windows",
            "platformOs": "windows"
        }
    }"#
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
    attempts: VecDeque<Result<FakeChild, String>>,
}

impl FakeLauncher {
    fn succeeding(response: String) -> Self {
        Self::with_attempts([Ok(FakeChild::succeeding(response))])
    }

    fn failing(message: &str) -> Self {
        Self::with_attempts([Err(message.to_string())])
    }

    fn with_attempts(attempts: impl IntoIterator<Item = Result<FakeChild, String>>) -> Self {
        Self {
            attempts: attempts.into_iter().collect(),
        }
    }
}

impl ProcessLauncher for FakeLauncher {
    fn launch(&mut self) -> Result<Box<dyn ProcessChild>, String> {
        match self.attempts.pop_front() {
            Some(Ok(child)) => Ok(Box::new(child)),
            Some(Err(error)) => Err(error),
            None => Err("unexpected launch attempt".to_string()),
        }
    }
}

struct FakeChild {
    reads: VecDeque<Result<String, ChildReadError>>,
    writes: Arc<Mutex<Vec<String>>>,
    terminated: Arc<AtomicBool>,
}

impl FakeChild {
    fn succeeding(response: String) -> Self {
        Self::new(
            [Ok(response)],
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn new(
        reads: impl IntoIterator<Item = Result<String, ChildReadError>>,
        writes: Arc<Mutex<Vec<String>>>,
        terminated: Arc<AtomicBool>,
    ) -> Self {
        Self {
            reads: reads.into_iter().collect(),
            writes,
            terminated,
        }
    }
}

impl ProcessChild for FakeChild {
    fn write(&mut self, message: &str) -> Result<(), String> {
        self.writes.lock().unwrap().push(message.to_string());
        Ok(())
    }

    fn receive_line(&mut self, _timeout: Duration) -> Result<String, ChildReadError> {
        self.reads
            .pop_front()
            .unwrap_or(Err(ChildReadError::Timeout))
    }

    fn terminate(&mut self) -> Result<(), String> {
        self.terminated.store(true, Ordering::SeqCst);
        Ok(())
    }
}
