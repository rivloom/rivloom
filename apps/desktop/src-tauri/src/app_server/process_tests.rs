use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::AppServerSupervisor;
use super::StatusObserver;
use crate::app_server::connection::ConnectionControl;
use crate::app_server::connection::ConnectionError;
use crate::app_server::connection::NotificationObserver;
use crate::app_server::protocol::initialize_request;
use crate::app_server::protocol::initialized_notification;
use crate::app_server::transport::ProcessControl;
use crate::app_server::transport::ProcessLauncher;
use crate::app_server::transport::ProcessTransport;
use crate::app_server::transport::TransportEvent;
use crate::runtime_status::RuntimeStatus;

const STARTUP_ERROR_MESSAGE: &str = "核心服务暂时无法启动。";
const TEST_WAIT_TIMEOUT: Duration = Duration::from_secs(/*secs*/ 1);

#[test]
fn successful_start_transitions_from_stopped_through_starting_to_connected() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, _handle) = FakeProcess::with_initialize_response();
    let mut supervisor =
        supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer.clone());

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
        FakeLauncher::with_attempts([Err("sidecar could not start".to_string())]),
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
fn successful_handshake_keeps_initialize_at_zero_then_writes_initialized() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, handle) = FakeProcess::with_initialize_response();
    let mut supervisor = supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer);

    assert_eq!(supervisor.start(), connected_status());
    assert_eq!(
        handle.writes(),
        vec![
            initialize_request().unwrap(),
            initialized_notification().unwrap(),
        ]
    );
    let initialize: Value = serde_json::from_str(&handle.writes()[0]).unwrap();
    assert_eq!(initialize["id"], json!(0));
}

#[test]
fn post_initialize_response_is_delivered_to_the_waiting_request() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, handle) = FakeProcess::with_initialize_response();
    let mut supervisor = supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer);
    assert_eq!(supervisor.start(), connected_status());

    let connection = supervisor.connection().expect("connected handle");
    let request =
        thread::spawn(move || connection.request("account/read", json!({ "refreshToken": false })));
    wait_until(|| handle.writes().len() == 3);

    assert!(handle.send_json(json!({
        "id": 1,
        "result": { "account": null }
    })));
    assert_eq!(request.join().unwrap(), Ok(json!({ "account": null })));
}

#[test]
fn post_initialize_notification_is_delivered_to_the_observer() {
    let observer = Arc::new(RecordingObserver::default());
    let notifications = Arc::new(RecordingNotificationObserver::default());
    let (process, handle) = FakeProcess::with_initialize_response();
    let mut supervisor = supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer);
    assert_eq!(supervisor.start(), connected_status());

    let connection = supervisor.connection().expect("connected handle");
    connection.set_notification_observer(notifications.clone());
    assert!(handle.send_json(json!({
        "method": "account/updated",
        "params": { "authMode": "chatgpt" }
    })));
    wait_until(|| !notifications.notifications().is_empty());

    assert_eq!(
        notifications.notifications(),
        vec![(
            "account/updated".to_string(),
            json!({ "authMode": "chatgpt" }),
        )]
    );
}

#[test]
fn process_termination_fails_pending_request_and_enters_runtime_error() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, handle) = FakeProcess::with_initialize_response();
    let mut supervisor =
        supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer.clone());
    assert_eq!(supervisor.start(), connected_status());

    let connection = supervisor.connection().expect("connected handle");
    let request = thread::spawn(move || connection.request("account/read", json!({})));
    wait_until(|| handle.writes().len() == 3);
    assert!(handle.send_event(TransportEvent::Terminated(Some(23))));

    assert_eq!(request.join().unwrap(), Err(ConnectionError::Disconnected));
    wait_until(|| observer.statuses().last() == Some(&startup_error_status()));
    assert!(supervisor.connection().is_none());
}

#[test]
fn retry_after_termination_uses_a_new_connection() {
    let observer = Arc::new(RecordingObserver::default());
    let (first_process, first_handle) = FakeProcess::with_initialize_response();
    let (second_process, second_handle) = FakeProcess::with_initialize_response();
    let mut supervisor = supervisor_with(
        FakeLauncher::with_attempts([Ok(first_process), Ok(second_process)]),
        observer.clone(),
    );
    assert_eq!(supervisor.start(), connected_status());
    let old_connection = supervisor.connection().expect("first connection");

    assert!(first_handle.send_event(TransportEvent::Terminated(Some(1))));
    wait_until(|| observer.statuses().last() == Some(&startup_error_status()));
    assert_eq!(supervisor.retry(), connected_status());
    assert_eq!(
        old_connection.request("account/read", json!({})),
        Err(ConnectionError::Disconnected)
    );

    let new_connection = supervisor.connection().expect("second connection");
    let request = thread::spawn(move || new_connection.request("account/read", json!({})));
    wait_until(|| second_handle.writes().len() == 3);
    assert!(second_handle.send_json(json!({
        "id": 1,
        "result": { "account": null }
    })));
    assert_eq!(request.join().unwrap(), Ok(json!({ "account": null })));
    assert_eq!(first_handle.writes().len(), 2);
}

#[test]
fn shutdown_stops_reader_and_terminates_the_process_once() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, handle) = FakeProcess::with_initialize_response();
    let mut supervisor =
        supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer.clone());
    assert_eq!(supervisor.start(), connected_status());

    assert_eq!(supervisor.shutdown(), RuntimeStatus::Stopped);
    assert_eq!(supervisor.shutdown(), RuntimeStatus::Stopped);
    assert_eq!(handle.terminate_calls(), 1);
    assert!(!handle.send_json(json!({
        "method": "account/updated",
        "params": {}
    })));
    assert!(supervisor.connection().is_none());
    assert_eq!(observer.statuses().last(), Some(&RuntimeStatus::Stopped));
}

#[test]
fn manual_retry_transitions_from_launch_error_to_connected() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, _handle) = FakeProcess::with_initialize_response();
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
fn initialization_timeout_transitions_to_error_and_terminates_the_process() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, handle) = FakeProcess::without_events();
    let mut supervisor =
        supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer.clone());

    assert_eq!(supervisor.start(), startup_error_status());
    assert_eq!(handle.terminate_calls(), 1);
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

fn success_response() -> Value {
    json!({
        "id": 0,
        "result": {
            "userAgent": "codex-app-server/1.2.3",
            "codexHome": "C:\\Users\\demo\\Rivloom\\codex-home",
            "platformFamily": "windows",
            "platformOs": "windows"
        }
    })
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

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + TEST_WAIT_TIMEOUT;
    while !predicate() {
        assert!(Instant::now() < deadline, "condition was not met in time");
        thread::sleep(Duration::from_millis(/*millis*/ 5));
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

#[derive(Default)]
struct RecordingNotificationObserver {
    notifications: Mutex<Vec<(String, Value)>>,
}

impl RecordingNotificationObserver {
    fn notifications(&self) -> Vec<(String, Value)> {
        self.notifications.lock().unwrap().clone()
    }
}

impl NotificationObserver for RecordingNotificationObserver {
    fn on_notification(&self, method: &str, params: &Value) {
        self.notifications
            .lock()
            .unwrap()
            .push((method.to_string(), params.clone()));
    }
}

struct FakeLauncher {
    attempts: VecDeque<Result<ProcessTransport, String>>,
}

impl FakeLauncher {
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
    fn with_initialize_response() -> (ProcessTransport, FakeProcessHandle) {
        let (transport, handle) = Self::without_events();
        assert!(handle.send_json(success_response()));
        (transport, handle)
    }

    fn without_events() -> (ProcessTransport, FakeProcessHandle) {
        let (events, receiver) = mpsc::channel();
        let control = Arc::new(FakeProcessControl {
            writes: Mutex::new(Vec::new()),
            terminate_calls: AtomicUsize::new(0),
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

    fn terminate_calls(&self) -> usize {
        self.control.terminate_calls.load(Ordering::SeqCst)
    }

    fn send_json(&self, value: Value) -> bool {
        self.send_event(TransportEvent::Stdout(format!("{value}\n").into_bytes()))
    }

    fn send_event(&self, event: TransportEvent) -> bool {
        self.control.events.send(event).is_ok()
    }
}

struct FakeProcessControl {
    writes: Mutex<Vec<String>>,
    terminate_calls: AtomicUsize,
    events: Sender<TransportEvent>,
}

impl ProcessControl for FakeProcessControl {
    fn write(&self, message: &str) -> Result<(), String> {
        self.writes.lock().unwrap().push(message.to_string());
        Ok(())
    }

    fn terminate(&self) -> Result<(), String> {
        self.terminate_calls.fetch_add(1, Ordering::SeqCst);
        let _ = self.events.send(TransportEvent::Terminated(Some(0)));
        Ok(())
    }
}
