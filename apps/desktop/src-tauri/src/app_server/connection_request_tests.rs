use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

use pretty_assertions::assert_eq;
use pretty_assertions::assert_ne;
use serde_json::Value;
use serde_json::json;

use super::AppServerConnection;
use super::ConnectionControl;
use super::ConnectionError;
use super::ConnectionOptions;
use super::InboundMessage;
use super::NotificationObserver;

const TEST_TIMEOUT: Duration = Duration::from_secs(/*secs*/ 5);

#[test]
fn connection_identity_is_stable_for_clones_and_unique_between_connections() {
    let first = AppServerConnection::new(|_| Ok(()));
    let first_clone = first.clone();
    let second = AppServerConnection::new(|_| Ok(()));

    assert_eq!(
        first.connection_identity(),
        first_clone.connection_identity()
    );
    assert_ne!(first.connection_identity(), second.connection_identity());
}

#[test]
fn requests_use_unique_ids_and_unknown_responses_do_not_disturb_waiters() {
    let harness = TestHarness::default();
    let first = spawn_request(harness.connection.clone(), "first/read");
    let first_write = harness.next_write();
    let first_id = request_id(&first_write);
    let second = spawn_request(harness.connection.clone(), "second/read");
    let second_write = harness.next_write();
    let second_id = request_id(&second_write);
    assert_eq!((first_id, second_id), (1, 2));
    assert_eq!(
        (first_write, second_write),
        (
            json!({"method": "first/read", "id": 1, "params": {}}),
            json!({"method": "second/read", "id": 2, "params": {}}),
        )
    );

    harness.respond(/*id*/ 999, json!({"unknown": true}));
    assert_eq!(first.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(second.try_recv(), Err(TryRecvError::Empty));

    harness.respond(second_id, json!({"order": 2}));
    assert_eq!(
        second.recv_timeout(TEST_TIMEOUT).unwrap(),
        Ok(json!({"order": 2}))
    );
    assert_eq!(first.try_recv(), Err(TryRecvError::Empty));
    harness.respond(first_id, json!({"order": 1}));
    assert_eq!(
        first.recv_timeout(TEST_TIMEOUT).unwrap(),
        Ok(json!({"order": 1}))
    );

    harness.respond(first_id, json!({"duplicate": true}));
}

#[test]
fn notifications_do_not_consume_pending_responses() {
    let harness = TestHarness::default();
    let notifications = Arc::new(CountingObserver::default());
    harness
        .connection
        .set_notification_observer(notifications.clone());
    let request = spawn_request(harness.connection.clone(), "account/read");
    let id = request_id(&harness.next_write());

    harness
        .connection
        .handle_inbound(InboundMessage::Notification {
            method: "account/updated".to_string(),
            params: json!({}),
        })
        .unwrap();
    assert_eq!(notifications.count.load(Ordering::SeqCst), 1);
    assert_eq!(request.try_recv(), Err(TryRecvError::Empty));

    harness.respond(id, json!({"account": null}));
    assert_eq!(
        request.recv_timeout(TEST_TIMEOUT).unwrap(),
        Ok(json!({"account": null}))
    );
}

#[test]
fn parameterless_requests_omit_the_params_field() {
    let harness = TestHarness::default();
    let request = spawn_parameterless_request(harness.connection.clone(), "account/logout");
    let write = harness.next_write();
    let id = request_id(&write);
    assert_eq!(write, json!({ "method": "account/logout", "id": 1 }));

    harness.respond(id, json!({}));
    assert_eq!(request.recv_timeout(TEST_TIMEOUT).unwrap(), Ok(json!({})));
}

#[test]
fn remote_errors_are_sanitized() {
    let harness = TestHarness::default();
    let request = spawn_request(harness.connection.clone(), "account/read");
    let id = request_id(&harness.next_write());

    harness
        .connection
        .handle_inbound(InboundMessage::ResponseError {
            id,
            code: -32600,
            message: "TOP_SECRET".to_string(),
        })
        .unwrap();

    let error = request.recv_timeout(TEST_TIMEOUT).unwrap().unwrap_err();
    assert_eq!(error, ConnectionError::Remote { code: -32600 });
    assert!(!error.to_string().contains("TOP_SECRET"));
}

#[test]
fn timeout_and_write_failure_remove_pending_requests() {
    let timeout_options = ConnectionOptions {
        request_timeout: Duration::from_millis(/*millis*/ 10),
        max_pending_requests: 1,
    };
    let timeout_harness = TestHarness::with_options(timeout_options);
    assert_eq!(
        timeout_harness.connection.request("first/read", json!({})),
        Err(ConnectionError::Timeout)
    );
    assert_eq!(
        timeout_harness.connection.request("second/read", json!({})),
        Err(ConnectionError::Timeout)
    );

    let write_failure = AppServerConnection::with_options(
        |_| Err("TOP_SECRET".to_string()),
        ConnectionOptions {
            request_timeout: TEST_TIMEOUT,
            max_pending_requests: 1,
        },
    );
    assert_eq!(
        write_failure.request("first/read", json!({})),
        Err(ConnectionError::WriteFailed)
    );
    assert_eq!(
        write_failure.request("second/read", json!({})),
        Err(ConnectionError::WriteFailed)
    );
}

#[test]
fn pending_limit_and_disconnect_are_enforced() {
    let harness = TestHarness::with_options(ConnectionOptions {
        request_timeout: TEST_TIMEOUT,
        max_pending_requests: 2,
    });
    let first = spawn_request(harness.connection.clone(), "first/read");
    harness.next_write();
    let second = spawn_request(harness.connection.clone(), "second/read");
    harness.next_write();

    assert_eq!(
        harness.connection.request("overflow/read", json!({})),
        Err(ConnectionError::TooManyPending)
    );
    harness.connection.disconnect();
    harness.connection.disconnect();
    assert_eq!(
        first.recv_timeout(TEST_TIMEOUT).unwrap(),
        Err(ConnectionError::Disconnected)
    );
    assert_eq!(
        second.recv_timeout(TEST_TIMEOUT).unwrap(),
        Err(ConnectionError::Disconnected)
    );
    assert_eq!(
        harness.connection.request("after/read", json!({})),
        Err(ConnectionError::Disconnected)
    );
}

#[test]
fn default_pending_limit_is_64() {
    let harness = TestHarness::default();
    let mut requests = Vec::new();
    for _ in 0..64 {
        requests.push(spawn_request(harness.connection.clone(), "bounded/read"));
        harness.next_write();
    }

    assert_eq!(
        harness.connection.request("overflow/read", json!({})),
        Err(ConnectionError::TooManyPending)
    );
    harness.connection.disconnect();
    for request in requests {
        assert_eq!(
            request.recv_timeout(TEST_TIMEOUT).unwrap(),
            Err(ConnectionError::Disconnected)
        );
    }
}

#[test]
fn exhausted_request_ids_fail_before_writing() {
    let harness = TestHarness::default();
    harness
        .connection
        .inner
        .next_request_id
        .store(u64::MAX, Ordering::Relaxed);

    assert_eq!(
        harness.connection.request("account/read", json!({})),
        Err(ConnectionError::RequestIdExhausted)
    );
    assert_eq!(harness.writes.try_recv(), Err(TryRecvError::Empty));
}

fn spawn_request(
    connection: AppServerConnection,
    method: &'static str,
) -> Receiver<Result<Value, ConnectionError>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(connection.request(method, json!({})));
    });
    receiver
}

fn spawn_parameterless_request(
    connection: AppServerConnection,
    method: &'static str,
) -> Receiver<Result<Value, ConnectionError>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(connection.request_without_params(method));
    });
    receiver
}

fn request_id(request: &Value) -> u64 {
    request["id"].as_u64().expect("numeric request ID")
}

struct TestHarness {
    connection: AppServerConnection,
    writes: Receiver<String>,
}

impl TestHarness {
    fn with_options(options: ConnectionOptions) -> Self {
        let (sender, writes) = mpsc::channel();
        let connection = AppServerConnection::with_options(
            move |line| {
                sender
                    .send(line.to_string())
                    .map_err(|error| error.to_string())
            },
            options,
        );
        Self { connection, writes }
    }

    fn next_write(&self) -> Value {
        let line = self.writes.recv_timeout(TEST_TIMEOUT).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn respond(&self, id: u64, result: Value) {
        self.connection
            .handle_inbound(InboundMessage::Response { id, result })
            .unwrap();
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::with_options(ConnectionOptions {
            request_timeout: TEST_TIMEOUT,
            max_pending_requests: 64,
        })
    }
}

#[derive(Default)]
struct CountingObserver {
    count: AtomicUsize,
}

impl NotificationObserver for CountingObserver {
    fn on_notification(
        &self,
        _connection_identity: &super::ConnectionIdentity,
        _method: &str,
        _params: &Value,
    ) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}
