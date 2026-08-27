use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use serde_json::Value;
use serde_json::json;

use super::AppServerSupervisor;
use super::StatusObserver;
use crate::app_server::connection::ConnectionControl;
use crate::app_server::connection::ConnectionError;
use crate::app_server::connection::ConnectionIdentity;
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
    let (process, _handle) = FakeProcess::with_response(success_response());
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
fn shutdown_disconnects_pending_requests_and_terminates_the_process_once() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, handle) = FakeProcess::with_response(success_response());
    let mut supervisor =
        supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer.clone());

    assert_eq!(supervisor.start(), connected_status());
    let connection = supervisor.connection().expect("connected handle");
    let pending_connection = connection.clone();
    let pending = thread::spawn(move || pending_connection.request("account/read", json!({})));
    wait_until(|| handle.writes().len() == 3);

    assert_eq!(supervisor.shutdown(), RuntimeStatus::Stopped);
    assert_eq!(pending.join().unwrap(), Err(ConnectionError::Disconnected));
    assert_eq!(
        connection.request("account/read", json!({})),
        Err(ConnectionError::Disconnected)
    );
    assert_eq!(supervisor.shutdown(), RuntimeStatus::Stopped);
    assert_eq!(handle.terminate_calls(), 1);
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
    let initialize: Value = serde_json::from_str(&handle.writes()[0]).unwrap();
    assert_eq!(initialize["id"], json!(0));
}

#[test]
fn coalesced_initialize_response_and_notification_are_both_consumed() {
    let observer = Arc::new(RecordingObserver::default());
    let notifications = Arc::new(RecordingNotificationObserver::default());
    let (process, handle) = FakeProcess::without_events();
    handle
        .send_stdout(format!(
            "{}\n{}\n",
            success_response(),
            json!({
                "method": "account/updated",
                "params": { "authMode": "chatgpt" }
            })
        ))
        .unwrap();
    let mut supervisor = supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer);
    supervisor.set_notification_observer(notifications.clone());

    assert_eq!(supervisor.start(), connected_status());
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
fn post_initialize_response_is_delivered_to_the_waiting_request() {
    let observer = Arc::new(RecordingObserver::default());
    let (process, handle) = FakeProcess::with_response(success_response());
    let mut supervisor = supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer);
    assert_eq!(supervisor.start(), connected_status());

    let connection = supervisor.connection().expect("connected handle");
    let request =
        thread::spawn(move || connection.request("account/read", json!({ "refreshToken": false })));
    wait_until(|| handle.writes().len() == 3);
    handle
        .send_json(json!({
            "id": 1,
            "result": { "account": null }
        }))
        .unwrap();

    assert_eq!(request.join().unwrap(), Ok(json!({ "account": null })));
}

#[test]
fn retry_after_termination_uses_a_new_connection() {
    let observer = Arc::new(RecordingObserver::default());
    let (first_process, first_handle) = FakeProcess::with_response(success_response());
    let (second_process, second_handle) = FakeProcess::with_response(success_response());
    let mut supervisor = supervisor_with(
        FakeLauncher::with_attempts([Ok(first_process), Ok(second_process)]),
        observer.clone(),
    );
    assert_eq!(supervisor.start(), connected_status());
    let old_connection = supervisor.connection().expect("first connection");

    first_handle
        .send_event(TransportEvent::Terminated(Some(1)))
        .unwrap();
    wait_until(|| observer.statuses().last() == Some(&startup_error_status()));
    assert_eq!(supervisor.retry(), connected_status());
    assert_eq!(
        old_connection.request("account/read", json!({})),
        Err(ConnectionError::Disconnected)
    );

    let new_connection = supervisor.connection().expect("second connection");
    let request = thread::spawn(move || new_connection.request("account/read", json!({})));
    wait_until(|| second_handle.writes().len() == 3);
    second_handle
        .send_json(json!({
            "id": 1,
            "result": { "account": null }
        }))
        .unwrap();
    assert_eq!(request.join().unwrap(), Ok(json!({ "account": null })));
    assert_eq!(first_handle.terminate_calls(), 0);
}

#[test]
fn invalid_runtime_json_disconnects_pending_and_terminates() {
    assert_reader_failure(
        |handle| handle.send_stdout("not-json\n".to_string()).unwrap(),
        /*expected_terminate_calls*/ 1,
    );
}

#[test]
fn runtime_transport_error_disconnects_pending_and_terminates() {
    assert_reader_failure(
        |handle| {
            handle
                .send_event(TransportEvent::Error("read failed".to_string()))
                .unwrap()
        },
        /*expected_terminate_calls*/ 1,
    );
}

#[test]
fn closed_runtime_channel_disconnects_pending_and_terminates() {
    assert_reader_failure(drop, /*expected_terminate_calls*/ 1);
}

#[test]
fn server_request_write_failure_disconnects_pending_and_terminates() {
    assert_reader_failure(
        |handle| {
            handle.fail_writes();
            handle
                .send_json(json!({
                    "id": "server-1",
                    "method": "item/tool/call",
                    "params": {}
                }))
                .unwrap();
        },
        /*expected_terminate_calls*/ 1,
    );
}

#[test]
fn process_termination_disconnects_pending_without_terminating_again() {
    assert_reader_failure(
        |handle| {
            handle
                .send_event(TransportEvent::Terminated(Some(23)))
                .unwrap()
        },
        /*expected_terminate_calls*/ 0,
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

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + TEST_WAIT_TIMEOUT;
    while !predicate() {
        assert!(Instant::now() < deadline, "condition was not met in time");
        thread::sleep(Duration::from_millis(/*millis*/ 5));
    }
}

fn assert_reader_failure(trigger: impl FnOnce(FakeProcessHandle), expected_terminate_calls: usize) {
    let observer = Arc::new(RecordingObserver::default());
    let (process, handle) = FakeProcess::with_response(success_response());
    let control = handle.control.clone();
    let mut supervisor =
        supervisor_with(FakeLauncher::with_attempts([Ok(process)]), observer.clone());
    assert_eq!(supervisor.start(), connected_status());

    let connection = supervisor.connection().expect("connected handle");
    let pending = thread::spawn(move || connection.request("account/read", json!({})));
    wait_until(|| handle.writes().len() == 3);
    trigger(handle);

    assert_eq!(pending.join().unwrap(), Err(ConnectionError::Disconnected));
    wait_until(|| observer.statuses().last() == Some(&startup_error_status()));
    assert!(supervisor.connection().is_none());
    assert_eq!(
        control.terminate_calls.load(Ordering::SeqCst),
        expected_terminate_calls
    );
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
    fn on_notification(
        &self,
        _connection_identity: &ConnectionIdentity,
        method: &str,
        params: &Value,
    ) {
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
            fail_writes: AtomicBool::new(false),
            terminate_calls: AtomicUsize::new(0),
        });
        let handle = FakeProcessHandle {
            control: control.clone(),
            events,
        };
        (ProcessTransport::new(control, receiver), handle)
    }
}

#[derive(Clone)]
struct FakeProcessHandle {
    control: Arc<FakeProcessControl>,
    events: mpsc::Sender<TransportEvent>,
}

impl FakeProcessHandle {
    fn writes(&self) -> Vec<String> {
        self.control.writes.lock().unwrap().clone()
    }

    fn terminate_calls(&self) -> usize {
        self.control.terminate_calls.load(Ordering::SeqCst)
    }

    fn fail_writes(&self) {
        self.control.fail_writes.store(true, Ordering::SeqCst);
    }

    fn send_json(&self, value: Value) -> Result<(), String> {
        self.send_stdout(format!("{value}\n"))
    }

    fn send_stdout(&self, stdout: String) -> Result<(), String> {
        self.send_event(TransportEvent::Stdout(stdout.into_bytes()))
    }

    fn send_event(&self, event: TransportEvent) -> Result<(), String> {
        self.events.send(event).map_err(|error| error.to_string())
    }
}

struct FakeProcessControl {
    writes: Mutex<Vec<String>>,
    fail_writes: AtomicBool,
    terminate_calls: AtomicUsize,
}

impl ProcessControl for FakeProcessControl {
    fn write(&self, message: &str) -> Result<(), String> {
        if self.fail_writes.load(Ordering::SeqCst) {
            return Err("write failed".to_string());
        }
        self.writes.lock().unwrap().push(message.to_string());
        Ok(())
    }

    fn terminate(&self) -> Result<(), String> {
        self.terminate_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
