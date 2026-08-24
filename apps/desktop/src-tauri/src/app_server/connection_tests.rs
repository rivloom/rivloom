use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::AppServerConnection;
use super::ConnectionControl;
use super::ConnectionError;
use super::ConnectionOptions;
use super::MAX_PENDING_REQUESTS;
use super::NotificationObserver;
use crate::app_server::wire::InboundMessage;

const TEST_WAIT: Duration = Duration::from_secs(/*secs*/ 1);
const SHORT_TIMEOUT: Duration = Duration::from_millis(/*millis*/ 25);

#[test]
fn request_ids_start_at_one_and_requests_are_jsonl() {
    let harness = TestHarness::default();
    let request = spawn_request(
        harness.connection.clone(),
        "account/read",
        json!({"refresh": true}),
    );

    let written = harness.next_write();
    assert_eq!(
        written,
        json!({
            "method": "account/read",
            "id": 1,
            "params": {"refresh": true},
        })
    );

    harness.respond(/*id*/ 1, json!({"account": null}));
    assert_eq!(request.join().unwrap(), Ok(json!({"account": null})));
}

#[test]
fn reversed_responses_reach_the_correct_callers() {
    let harness = TestHarness::default();
    let first = spawn_request(harness.connection.clone(), "first/read", json!({"slot": 1}));
    let first_id = request_id(&harness.next_write());
    let second = spawn_request(
        harness.connection.clone(),
        "second/read",
        json!({"slot": 2}),
    );
    let second_id = request_id(&harness.next_write());

    harness.respond(second_id, json!({"owner": "second"}));
    harness.respond(first_id, json!({"owner": "first"}));

    assert_eq!(first.join().unwrap(), Ok(json!({"owner": "first"})));
    assert_eq!(second.join().unwrap(), Ok(json!({"owner": "second"})));
}

#[test]
fn notifications_do_not_consume_pending_responses() {
    let harness = TestHarness::default();
    let request = spawn_request(harness.connection.clone(), "account/read", json!({}));
    let request_id = request_id(&harness.next_write());

    harness
        .connection
        .handle_inbound(InboundMessage::Notification {
            method: "account/updated".to_string(),
            params: json!({"authMode": "chatgpt"}),
        })
        .unwrap();

    assert!(!request.is_finished());
    assert_eq!(
        harness.observer.notifications(),
        vec![(
            "account/updated".to_string(),
            json!({"authMode": "chatgpt"}),
        )]
    );

    harness.respond(request_id, json!({"account": null}));
    assert_eq!(request.join().unwrap(), Ok(json!({"account": null})));
}

#[test]
fn remote_errors_discard_the_server_message() {
    let harness = TestHarness::default();
    let request = spawn_request(harness.connection.clone(), "account/read", json!({}));
    let request_id = request_id(&harness.next_write());

    harness
        .connection
        .handle_inbound(InboundMessage::ResponseError {
            id: request_id,
            code: 401,
            message: "TOP_SECRET remote detail".to_string(),
        })
        .unwrap();

    let error = request.join().unwrap().unwrap_err();
    assert_eq!(error, ConnectionError::Remote { code: 401 });
    assert!(!error.to_string().contains("TOP_SECRET"));
    assert!(!format!("{error:?}").contains("TOP_SECRET"));
}

#[test]
fn timeout_removes_the_pending_request() {
    let harness = TestHarness::with_options(ConnectionOptions {
        request_timeout: SHORT_TIMEOUT,
        max_pending_requests: 1,
    });

    assert_eq!(
        ConnectionControl::request(&harness.connection, "slow/read", json!({})),
        Err(ConnectionError::Timeout)
    );
    let timed_out_id = request_id(&harness.next_write());
    harness.respond(timed_out_id, json!({"late": true}));

    let retry = spawn_request(harness.connection.clone(), "retry/read", json!({}));
    let retry_id = request_id(&harness.next_write());
    harness.respond(retry_id, json!({"ok": true}));
    assert_eq!(retry.join().unwrap(), Ok(json!({"ok": true})));
}

#[test]
fn write_failure_removes_the_pending_request_and_detail() {
    let harness = TestHarness::with_options(ConnectionOptions {
        request_timeout: TEST_WAIT,
        max_pending_requests: 1,
    });
    harness.fail_writes.store(/*val*/ true, Ordering::Relaxed);

    let error =
        ConnectionControl::request(&harness.connection, "account/read", json!({})).unwrap_err();

    assert_eq!(error, ConnectionError::WriteFailed);
    assert!(!error.to_string().contains("TOP_SECRET"));

    harness.fail_writes.store(/*val*/ false, Ordering::Relaxed);
    let retry = spawn_request(harness.connection.clone(), "retry/read", json!({}));
    let retry_id = request_id(&harness.next_write());
    harness.respond(retry_id, json!({"ok": true}));
    assert_eq!(retry.join().unwrap(), Ok(json!({"ok": true})));
}

#[test]
fn sixty_fifth_pending_request_is_rejected() {
    let harness = TestHarness::default();
    let mut requests = Vec::new();

    for slot in 0..MAX_PENDING_REQUESTS {
        requests.push(spawn_request(
            harness.connection.clone(),
            "slot/read",
            json!({"slot": slot}),
        ));
        harness.next_write();
    }

    assert_eq!(
        ConnectionControl::request(&harness.connection, "overflow/read", json!({})),
        Err(ConnectionError::TooManyPending)
    );
    assert_eq!(harness.writes.try_recv(), Err(TryRecvError::Empty));

    harness.connection.disconnect();
    for request in requests {
        assert_eq!(request.join().unwrap(), Err(ConnectionError::Disconnected));
    }
}

#[test]
fn disconnect_fails_each_waiter_once_and_rejects_new_requests() {
    let harness = TestHarness::default();
    let first = spawn_request(harness.connection.clone(), "first/read", json!({}));
    harness.next_write();
    let second = spawn_request(harness.connection.clone(), "second/read", json!({}));
    harness.next_write();

    harness.connection.disconnect();
    harness.connection.disconnect();

    assert_eq!(first.join().unwrap(), Err(ConnectionError::Disconnected));
    assert_eq!(second.join().unwrap(), Err(ConnectionError::Disconnected));
    assert_eq!(
        ConnectionControl::request(&harness.connection, "after/read", json!({})),
        Err(ConnectionError::Disconnected)
    );
    assert_eq!(harness.writes.try_recv(), Err(TryRecvError::Empty));
}

#[test]
fn duplicate_and_unknown_response_ids_are_ignored() {
    let harness = TestHarness::default();
    harness.respond(/*id*/ 999, json!({"unknown": true}));

    let request = spawn_request(harness.connection.clone(), "account/read", json!({}));
    let request_id = request_id(&harness.next_write());
    harness.respond(request_id, json!({"ok": true}));
    assert_eq!(request.join().unwrap(), Ok(json!({"ok": true})));

    harness.respond(request_id, json!({"duplicate": true}));
    harness.respond(/*id*/ 1_000, json!({"unknown": true}));
}

#[test]
fn server_requests_receive_method_not_supported() {
    let harness = TestHarness::default();

    harness
        .connection
        .handle_inbound(InboundMessage::ServerRequest {
            id: json!("approval-1"),
            method: "item/commandExecution/requestApproval".to_string(),
            params: json!({"command": "TOP_SECRET"}),
        })
        .unwrap();

    assert_eq!(
        harness.next_write(),
        json!({
            "id": "approval-1",
            "error": {
                "code": -32601,
                "message": "Method not supported",
            },
        })
    );
}

fn spawn_request(
    connection: AppServerConnection,
    method: &'static str,
    params: Value,
) -> JoinHandle<Result<Value, ConnectionError>> {
    thread::spawn(move || ConnectionControl::request(&connection, method, params))
}

fn request_id(request: &Value) -> u64 {
    request["id"].as_u64().unwrap()
}

#[derive(Default)]
struct RecordingObserver {
    notifications: Mutex<Vec<(String, Value)>>,
}

impl RecordingObserver {
    fn notifications(&self) -> Vec<(String, Value)> {
        self.notifications
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl NotificationObserver for RecordingObserver {
    fn on_notification(&self, method: &str, params: &Value) {
        self.notifications
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((method.to_string(), params.clone()));
    }
}

struct TestHarness {
    connection: AppServerConnection,
    writes: Receiver<String>,
    fail_writes: Arc<AtomicBool>,
    observer: Arc<RecordingObserver>,
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::with_options(ConnectionOptions::default())
    }
}

impl TestHarness {
    fn with_options(options: ConnectionOptions) -> Self {
        let (write_sender, writes) = mpsc::channel();
        let fail_writes = Arc::new(AtomicBool::new(/*v*/ false));
        let fail_writes_for_writer = fail_writes.clone();
        let connection = AppServerConnection::with_options(
            move |line| {
                if fail_writes_for_writer.load(Ordering::Relaxed) {
                    return Err("TOP_SECRET write detail".to_string());
                }
                write_sender
                    .send(line.to_string())
                    .map_err(|_| "test write receiver closed".to_string())
            },
            options,
        );
        let observer = Arc::new(RecordingObserver::default());
        connection.set_notification_observer(observer.clone());

        Self {
            connection,
            writes,
            fail_writes,
            observer,
        }
    }

    fn next_write(&self) -> Value {
        let line = self.writes.recv_timeout(TEST_WAIT).unwrap();
        let json = line
            .strip_suffix('\n')
            .expect("request must end in newline");
        serde_json::from_str(json).unwrap()
    }

    fn respond(&self, id: u64, result: Value) {
        self.connection
            .handle_inbound(InboundMessage::Response { id, result })
            .unwrap();
    }
}
