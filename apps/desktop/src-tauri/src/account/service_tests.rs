use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::AccountService;
use crate::account::types::AccountStatus;
use crate::app_server::ConnectionControl;
use crate::app_server::ConnectionError;

#[test]
fn account_read_maps_supported_and_unsupported_configurations() {
    let harness = Harness::new(vec![
        Ok(json!({ "account": null, "requiresOpenaiAuth": true })),
        Ok(json!({ "account": null, "requiresOpenaiAuth": false })),
        Ok(json!({
            "account": { "type": "chatgpt", "email": null, "planType": "plus" },
            "requiresOpenaiAuth": true,
        })),
        Ok(json!({
            "account": {
                "type": "chatgpt",
                "email": "user@example.com",
                "planType": "pro",
            },
            "requiresOpenaiAuth": true,
        })),
        Ok(json!({
            "account": { "type": "chatgpt", "planType": "plus" },
            "requiresOpenaiAuth": true,
        })),
        Ok(json!({
            "account": { "type": "apiKey" },
            "requiresOpenaiAuth": true,
        })),
        Ok(json!({
            "account": { "type": "amazonBedrock", "usesCodexManagedCredentials": false },
            "requiresOpenaiAuth": false,
        })),
    ]);

    assert_eq!(
        (0..7)
            .map(|_| harness.service.refresh())
            .collect::<Vec<_>>(),
        vec![
            AccountStatus::SignedOut,
            unsupported_account_error(),
            signed_in_status(),
            AccountStatus::SignedIn {
                email: Some("user@example.com".to_string()),
                plan_type: "pro".to_string(),
            },
            retryable_account_error(),
            unsupported_account_error(),
            unsupported_account_error(),
        ]
    );
    assert_eq!(
        harness.connection.requests(),
        vec![request("account/read", json!({ "refreshToken": false })); 7]
    );
}

#[test]
fn a_service_without_a_connection_has_explicit_safe_states() {
    let service = AccountService::new();

    assert_eq!(service.status(), AccountStatus::Checking);
    assert_eq!(service.refresh(), retryable_account_error());
    assert_eq!(service.status(), retryable_account_error());
}

#[test]
fn account_read_failures_and_malformed_fields_are_sanitized() {
    let harness = Harness::new(vec![
        Ok(json!({ "accessToken": "secret" })),
        Ok(json!({
            "account": { "type": "chatgpt", "email": 123, "planType": "plus" },
            "requiresOpenaiAuth": true,
        })),
        Err(ConnectionError::Disconnected),
    ]);

    assert_eq!(
        (0..3)
            .map(|_| harness.service.refresh())
            .collect::<Vec<_>>(),
        vec![retryable_account_error(); 3]
    );
    assert_eq!(harness.service.status(), retryable_account_error());
}

#[test]
fn disconnect_clears_state_and_reconnect_starts_from_checking() {
    let first_connection = Arc::new(FakeConnection::new(vec![signed_in_response()]));
    let service = AccountService::new();
    service.connect(first_connection);
    assert_eq!(service.refresh(), signed_in_status());

    assert_eq!(service.disconnect(), retryable_account_error());
    assert_eq!(service.status(), retryable_account_error());

    let second_connection = Arc::new(FakeConnection::new(vec![Ok(json!({
        "account": null,
        "requiresOpenaiAuth": true,
    }))]));
    assert_eq!(service.connect(second_connection), AccountStatus::Checking);
    assert_eq!(service.refresh(), AccountStatus::SignedOut);
}

#[test]
fn a_late_read_from_an_old_connection_cannot_overwrite_reconnected_state() {
    let service = AccountService::new();
    let (started_sender, started_receiver) = mpsc::channel();
    let (response_sender, response_receiver) = mpsc::channel();
    service.connect(Arc::new(BlockingConnection {
        started_sender,
        response_receiver: Mutex::new(response_receiver),
    }));

    let old_service = service.clone();
    let old_read = thread::spawn(move || old_service.refresh());
    started_receiver
        .recv_timeout(Duration::from_secs(/*secs*/ 1))
        .expect("old connection should receive account/read");

    let second_connection = Arc::new(FakeConnection::new(vec![Ok(json!({
        "account": null,
        "requiresOpenaiAuth": true,
    }))]));
    let reconnect_service = service.clone();
    let (reconnected_sender, reconnected_receiver) = mpsc::channel();
    let reconnect = thread::spawn(move || {
        let status = reconnect_service.connect(second_connection);
        reconnected_sender.send(status).unwrap();
    });
    let reconnect_status = match reconnected_receiver
        .recv_timeout(Duration::from_secs(/*secs*/ 1))
    {
        Ok(status) => status,
        Err(error) => {
            let _ = response_sender.send(signed_in_response());
            match reconnected_receiver.recv_timeout(Duration::from_secs(/*secs*/ 1)) {
                Ok(status) => {
                    reconnect.join().unwrap();
                    panic!(
                        "reconnect returned {status:?} only after the old request was released: {error}"
                    );
                }
                Err(release_error) => panic!(
                    "reconnect remained blocked after the old request was released: {error}; {release_error}"
                ),
            }
        }
    };
    reconnect.join().unwrap();
    assert_eq!(reconnect_status, AccountStatus::Checking);
    assert_eq!(service.refresh(), AccountStatus::SignedOut);
    response_sender.send(signed_in_response()).unwrap();

    assert_eq!(old_read.join().unwrap(), AccountStatus::SignedOut);
    assert_eq!(service.status(), AccountStatus::SignedOut);
}

#[test]
fn an_older_read_on_the_same_connection_cannot_overwrite_a_newer_result() {
    let service = AccountService::new();
    let (request_sender, request_receiver) = mpsc::channel();
    service.connect(Arc::new(ControlledConnection { request_sender }));

    let first_service = service.clone();
    let first_read = thread::spawn(move || first_service.refresh());
    let first_request = request_receiver
        .recv_timeout(Duration::from_secs(/*secs*/ 1))
        .expect("first account/read should arrive");
    assert_eq!(
        first_request.request,
        request("account/read", json!({ "refreshToken": false }))
    );

    let second_service = service.clone();
    let second_read = thread::spawn(move || second_service.refresh());
    let second_request = request_receiver
        .recv_timeout(Duration::from_secs(/*secs*/ 1))
        .expect("second account/read should arrive");
    assert_eq!(
        second_request.request,
        request("account/read", json!({ "refreshToken": false }))
    );

    second_request
        .response_sender
        .send(Ok(json!({
            "account": null,
            "requiresOpenaiAuth": true,
        })))
        .unwrap();
    assert_eq!(second_read.join().unwrap(), AccountStatus::SignedOut);

    first_request
        .response_sender
        .send(signed_in_response())
        .unwrap();
    assert_eq!(first_read.join().unwrap(), AccountStatus::SignedOut);
    assert_eq!(service.status(), AccountStatus::SignedOut);
}

struct Harness {
    service: AccountService,
    connection: Arc<FakeConnection>,
}

impl Harness {
    fn new(responses: Vec<Result<Value, ConnectionError>>) -> Self {
        let connection = Arc::new(FakeConnection::new(responses));
        let service = AccountService::new();
        service.connect(connection.clone());
        Self {
            service,
            connection,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct RecordedRequest {
    method: String,
    params: Value,
}

struct FakeConnection {
    responses: Mutex<VecDeque<Result<Value, ConnectionError>>>,
    requests: Mutex<Vec<RecordedRequest>>,
}

impl FakeConnection {
    fn new(responses: Vec<Result<Value, ConnectionError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl ConnectionControl for FakeConnection {
    fn request(&self, method: &str, params: Value) -> Result<Value, ConnectionError> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request(method, params));
        self.responses
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or(Err(ConnectionError::Disconnected))
    }
}

struct BlockingConnection {
    started_sender: mpsc::Sender<()>,
    response_receiver: Mutex<mpsc::Receiver<Result<Value, ConnectionError>>>,
}

struct ControlledRequest {
    request: RecordedRequest,
    response_sender: mpsc::Sender<Result<Value, ConnectionError>>,
}

struct ControlledConnection {
    request_sender: mpsc::Sender<ControlledRequest>,
}

impl ConnectionControl for ControlledConnection {
    fn request(&self, method: &str, params: Value) -> Result<Value, ConnectionError> {
        let (response_sender, response_receiver) = mpsc::channel();
        self.request_sender
            .send(ControlledRequest {
                request: request(method, params),
                response_sender,
            })
            .unwrap();
        response_receiver.recv().unwrap()
    }
}

impl ConnectionControl for BlockingConnection {
    fn request(&self, _method: &str, _params: Value) -> Result<Value, ConnectionError> {
        self.started_sender.send(()).unwrap();
        self.response_receiver
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv()
            .unwrap()
    }
}

fn request(method: &str, params: Value) -> RecordedRequest {
    RecordedRequest {
        method: method.to_string(),
        params,
    }
}

fn signed_in_response() -> Result<Value, ConnectionError> {
    Ok(json!({
        "account": { "type": "chatgpt", "email": null, "planType": "plus" },
        "requiresOpenaiAuth": true,
    }))
}

fn signed_in_status() -> AccountStatus {
    AccountStatus::SignedIn {
        email: None,
        plan_type: "plus".to_string(),
    }
}

fn retryable_account_error() -> AccountStatus {
    AccountStatus::Error {
        message: "账号状态暂时不可用。".to_string(),
        retryable: true,
    }
}

fn unsupported_account_error() -> AccountStatus {
    AccountStatus::Error {
        message: "当前核心服务配置不支持 ChatGPT 账号登录。".to_string(),
        retryable: false,
    }
}
