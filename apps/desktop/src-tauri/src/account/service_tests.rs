use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::TryLockError;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tauri::Url;

use super::AccountService;
use crate::account::login::UrlOpener;
use crate::account::types::AccountStatus;
use crate::app_server::ConnectionControl;
use crate::app_server::ConnectionError;
use crate::app_server::ConnectionIdentity;

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
        identity: ConnectionIdentity::new(),
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
    service.connect(Arc::new(ControlledConnection {
        identity: ConnectionIdentity::new(),
        request_sender,
    }));

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

#[test]
fn a_read_started_before_browser_login_cannot_overwrite_pending() {
    let (service, _, request_receiver) = controlled_browser_service();
    let read_service = service.clone();
    let read = spawn_status_task(move || read_service.refresh());
    let read_request = next_request(&request_receiver, "account/read should arrive");
    let login = spawn_browser_login(&service);
    next_request(&request_receiver, "account/login/start should arrive").respond(
        browser_start_request(),
        browser_login_response("browser-login", "https://auth.openai.com/oauth"),
    );
    assert_eq!(
        login.wait("browser login should finish"),
        AccountStatus::BrowserPending
    );
    read_request.respond(
        request("account/read", json!({ "refreshToken": false })),
        Ok(json!({
            "account": null,
            "requiresOpenaiAuth": true,
        })),
    );
    assert_eq!(
        (read.wait("account/read should finish"), service.status()),
        (AccountStatus::BrowserPending, AccountStatus::BrowserPending)
    );
}

#[test]
fn a_read_started_after_browser_login_cannot_overwrite_pending() {
    let (service, _, _) = browser_harness(
        vec![
            browser_login_response("browser-login", "https://auth.openai.com/oauth"),
            Ok(json!({
                "account": null,
                "requiresOpenaiAuth": true,
            })),
        ],
        vec![],
    );

    assert_eq!(service.start_browser_login(), AccountStatus::BrowserPending);
    assert_eq!(
        (service.refresh(), service.status()),
        (AccountStatus::BrowserPending, AccountStatus::BrowserPending)
    );
}

#[test]
fn browser_login_holds_the_serialization_gate_until_the_attempt_is_installed() {
    let (service, opener, request_receiver) = controlled_browser_service();
    let login = spawn_browser_login(&service);
    let request = next_request(&request_receiver, "login request should arrive");
    assert!(matches!(
        service.inner.login_operation.try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    request.respond(
        browser_start_request(),
        browser_login_response("browser-login", "https://auth.openai.com/oauth"),
    );
    assert_eq!(
        (
            login.wait("browser login should finish"),
            opener.opened_urls()
        ),
        (
            AccountStatus::BrowserPending,
            vec!["https://auth.openai.com/oauth".to_string()],
        )
    );
}

#[test]
fn a_valid_login_on_an_old_connection_is_canceled_without_opening() {
    let (service, opener, request_receiver) = controlled_browser_service();
    let old_login = spawn_browser_login(&service);
    let old_request = next_request(&request_receiver, "old login should reach the connection");
    service.connect(Arc::new(FakeConnection::new(vec![Ok(json!({
        "account": null,
        "requiresOpenaiAuth": true,
    }))])));
    assert_eq!(service.refresh(), AccountStatus::SignedOut);
    old_request.respond(
        browser_start_request(),
        browser_login_response("old-login", "https://auth.openai.com/old"),
    );
    next_request(&request_receiver, "old login should be canceled").respond(
        request("account/login/cancel", json!({ "loginId": "old-login" })),
        Ok(json!({ "status": "canceled" })),
    );
    assert_eq!(
        (
            old_login.wait("old login cleanup should finish"),
            service.status(),
            opener.opened_urls()
        ),
        (
            AccountStatus::SignedOut,
            AccountStatus::SignedOut,
            Vec::<String>::new()
        )
    );
}

#[test]
fn browser_open_holds_the_connection_transition_gate_until_it_finishes() {
    let (entered_sender, entered_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let service = AccountService::with_url_opener(Arc::new(BlockingUrlOpener {
        entered_sender: Mutex::new(Some(entered_sender)),
        release_receiver: Mutex::new(release_receiver),
    }));
    service.connect(Arc::new(FakeConnection::new(vec![browser_login_response(
        "browser-login",
        "https://auth.openai.com/oauth",
    )])));
    let login = spawn_browser_login(&service);
    entered_receiver
        .recv_timeout(Duration::from_secs(/*secs*/ 1))
        .expect("browser opener should be reached");

    assert!(matches!(
        service.inner.browser_open_operation.try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    release_sender.send(()).unwrap();
    let login_status = login.wait("browser login should finish after release");
    let reconnect_status = service.connect(Arc::new(FakeConnection::new(vec![])));

    assert_eq!(
        (login_status, reconnect_status, service.status()),
        (
            AccountStatus::BrowserPending,
            AccountStatus::Checking,
            AccountStatus::Checking
        )
    );
}

#[test]
fn failed_browser_cleanup_retains_the_attempt_for_the_next_retry() {
    let (service, connection, _) = browser_harness(
        vec![
            browser_login_response("first-login", "https://auth.openai.com/first"),
            Err(ConnectionError::Timeout),
            Ok(json!({ "status": "canceled" })),
            browser_login_response("second-login", "https://auth.openai.com/second"),
        ],
        vec![Err(())],
    );

    assert_eq!(service.start_browser_login(), browser_open_error());
    assert_eq!(service.start_browser_login(), AccountStatus::BrowserPending);
    assert_eq!(
        connection.requests(),
        vec![
            browser_start_request(),
            request("account/login/cancel", json!({ "loginId": "first-login" }),),
            request("account/login/cancel", json!({ "loginId": "first-login" }),),
            browser_start_request(),
        ]
    );
}

#[test]
fn invalid_browser_urls_are_canceled_and_suggest_device_code() {
    let (service, connection, opener) = browser_harness(
        vec![
            browser_login_response(
                "TOP_SECRET_LOGIN_ID",
                "https://openai.com.evil.example/oauth?secret=TOP_SECRET",
            ),
            Ok(json!({ "status": "notFound" })),
        ],
        vec![],
    );

    assert_eq!(service.start_browser_login(), browser_open_error());
    assert_eq!(opener.opened_urls(), Vec::<String>::new());
    assert_eq!(
        connection.requests(),
        vec![
            browser_start_request(),
            request(
                "account/login/cancel",
                json!({ "loginId": "TOP_SECRET_LOGIN_ID" }),
            ),
        ]
    );
    let serialized = serde_json::to_string(&service.status()).unwrap();
    assert!(!serialized.contains("TOP_SECRET"));
}

#[test]
fn browser_start_failures_are_retryable_and_do_not_open_or_leak() {
    let responses = vec![
        Err(ConnectionError::Timeout),
        Ok(json!({ "malformed": "TOP_SECRET" })),
        Ok(json!({ "type": "future", "secret": "TOP_SECRET" })),
        browser_login_response("", "https://auth.openai.com/?secret=TOP_SECRET"),
    ];

    for response in responses {
        let (service, _, opener) = browser_harness(vec![response], vec![]);
        assert_eq!(service.start_browser_login(), login_unavailable_error());
        assert_eq!(opener.opened_urls(), Vec::<String>::new());
        assert!(
            !serde_json::to_string(&service.status())
                .unwrap()
                .contains("TOP_SECRET")
        );
    }

    let service = AccountService::new();
    assert_eq!(service.start_browser_login(), login_unavailable_error());
}

#[test]
fn unexpected_device_responses_are_canceled_and_retained_until_cleanup() {
    let (service, connection, opener) = browser_harness(
        vec![
            device_login_response("first-device"),
            Ok(json!({ "status": "canceled" })),
            device_login_response("second-device"),
            Err(ConnectionError::Timeout),
            Ok(json!({ "status": "canceled" })),
            browser_login_response("browser-login", "https://auth.openai.com/oauth"),
        ],
        vec![],
    );

    assert_eq!(service.start_browser_login(), login_unavailable_error());
    assert_eq!(service.start_browser_login(), login_unavailable_error());
    assert_eq!(service.start_browser_login(), AccountStatus::BrowserPending);
    assert_eq!(
        (
            connection.requests(),
            opener.opened_urls(),
            service.status()
        ),
        (
            vec![
                browser_start_request(),
                request("account/login/cancel", json!({ "loginId": "first-device" }),),
                browser_start_request(),
                request(
                    "account/login/cancel",
                    json!({ "loginId": "second-device" }),
                ),
                request(
                    "account/login/cancel",
                    json!({ "loginId": "second-device" }),
                ),
                browser_start_request(),
            ],
            vec!["https://auth.openai.com/oauth".to_string()],
            AccountStatus::BrowserPending,
        )
    );
}

pub(super) fn controlled_browser_service() -> (
    AccountService,
    Arc<FakeUrlOpener>,
    mpsc::Receiver<ControlledRequest>,
) {
    let opener = Arc::new(FakeUrlOpener::new(vec![]));
    let service = AccountService::with_url_opener(opener.clone());
    let (request_sender, request_receiver) = mpsc::channel();
    service.connect(Arc::new(ControlledConnection {
        identity: ConnectionIdentity::new(),
        request_sender,
    }));
    (service, opener, request_receiver)
}

fn spawn_browser_login(service: &AccountService) -> StatusTask {
    let service = service.clone();
    spawn_status_task(move || service.start_browser_login())
}

pub(super) fn spawn_status_task(
    operation: impl FnOnce() -> AccountStatus + Send + 'static,
) -> StatusTask {
    let (result_sender, result_receiver) = mpsc::channel();
    let thread = thread::spawn(move || {
        let _ = result_sender.send(operation());
    });
    StatusTask {
        result_receiver,
        thread,
    }
}

pub(super) fn next_request(
    receiver: &mpsc::Receiver<ControlledRequest>,
    message: &str,
) -> ControlledRequest {
    receiver
        .recv_timeout(Duration::from_secs(/*secs*/ 1))
        .unwrap_or_else(|error| panic!("{message}: {error}"))
}

pub(super) struct StatusTask {
    result_receiver: mpsc::Receiver<AccountStatus>,
    thread: thread::JoinHandle<()>,
}

impl StatusTask {
    pub(super) fn wait(self, message: &str) -> AccountStatus {
        let status = self
            .result_receiver
            .recv_timeout(Duration::from_secs(/*secs*/ 1))
            .unwrap_or_else(|error| panic!("{message}: {error}"));
        self.thread.join().unwrap();
        status
    }
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

pub(super) fn browser_harness(
    responses: Vec<Result<Value, ConnectionError>>,
    open_results: Vec<Result<(), ()>>,
) -> (AccountService, Arc<FakeConnection>, Arc<FakeUrlOpener>) {
    let connection = Arc::new(FakeConnection::new(responses));
    let opener = Arc::new(FakeUrlOpener::new(open_results));
    let service = AccountService::with_url_opener(opener.clone());
    service.connect(connection.clone());
    (service, connection, opener)
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RecordedRequest {
    pub(super) method: String,
    pub(super) params: Value,
}

const OMITTED_PARAMS_SENTINEL: &str = "__omitted_params__";

pub(super) struct FakeConnection {
    identity: ConnectionIdentity,
    responses: Mutex<VecDeque<Result<Value, ConnectionError>>>,
    requests: Mutex<Vec<RecordedRequest>>,
}

impl FakeConnection {
    pub(super) fn new(responses: Vec<Result<Value, ConnectionError>>) -> Self {
        Self {
            identity: ConnectionIdentity::new(),
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn requests(&self) -> Vec<RecordedRequest> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl ConnectionControl for FakeConnection {
    fn connection_identity(&self) -> ConnectionIdentity {
        self.identity.clone()
    }

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
    fn request_without_params(&self, method: &str) -> Result<Value, ConnectionError> {
        self.request(method, json!(OMITTED_PARAMS_SENTINEL))
    }
}

struct BlockingConnection {
    identity: ConnectionIdentity,
    started_sender: mpsc::Sender<()>,
    response_receiver: Mutex<mpsc::Receiver<Result<Value, ConnectionError>>>,
}

pub(super) struct ControlledRequest {
    request: RecordedRequest,
    response_sender: mpsc::Sender<Result<Value, ConnectionError>>,
}

impl ControlledRequest {
    pub(super) fn respond(
        self,
        expected: RecordedRequest,
        response: Result<Value, ConnectionError>,
    ) {
        assert_eq!(self.request, expected);
        self.response_sender.send(response).unwrap();
    }
}

struct ControlledConnection {
    identity: ConnectionIdentity,
    request_sender: mpsc::Sender<ControlledRequest>,
}

pub(super) struct FakeUrlOpener {
    results: Mutex<VecDeque<Result<(), ()>>>,
    opened_urls: Mutex<Vec<String>>,
}

struct BlockingUrlOpener {
    entered_sender: Mutex<Option<mpsc::Sender<()>>>,
    release_receiver: Mutex<mpsc::Receiver<()>>,
}

impl UrlOpener for BlockingUrlOpener {
    fn open(&self, _url: &Url) -> Result<(), ()> {
        self.entered_sender
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .expect("opener should be called once")
            .send(())
            .unwrap();
        self.release_receiver
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv_timeout(Duration::from_secs(/*secs*/ 2))
            .map_err(|_| ())
    }
}

impl FakeUrlOpener {
    pub(super) fn new(results: Vec<Result<(), ()>>) -> Self {
        Self {
            results: Mutex::new(results.into()),
            opened_urls: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn opened_urls(&self) -> Vec<String> {
        self.opened_urls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl UrlOpener for FakeUrlOpener {
    fn open(&self, url: &Url) -> Result<(), ()> {
        self.opened_urls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(url.as_str().to_string());
        self.results
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or(Ok(()))
    }
}

impl ConnectionControl for ControlledConnection {
    fn connection_identity(&self) -> ConnectionIdentity {
        self.identity.clone()
    }

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

    fn request_without_params(&self, method: &str) -> Result<Value, ConnectionError> {
        self.request(method, json!(OMITTED_PARAMS_SENTINEL))
    }
}

impl ConnectionControl for BlockingConnection {
    fn connection_identity(&self) -> ConnectionIdentity {
        self.identity.clone()
    }

    fn request(&self, _method: &str, _params: Value) -> Result<Value, ConnectionError> {
        self.started_sender.send(()).unwrap();
        self.response_receiver
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv()
            .unwrap()
    }

    fn request_without_params(&self, method: &str) -> Result<Value, ConnectionError> {
        self.request(method, Value::Null)
    }
}

pub(super) fn request(method: &str, params: Value) -> RecordedRequest {
    RecordedRequest {
        method: method.to_string(),
        params,
    }
}

pub(super) fn request_without_params(method: &str) -> RecordedRequest {
    request(method, json!(OMITTED_PARAMS_SENTINEL))
}

pub(super) fn signed_in_response() -> Result<Value, ConnectionError> {
    Ok(json!({
        "account": { "type": "chatgpt", "email": null, "planType": "plus" },
        "requiresOpenaiAuth": true,
    }))
}

pub(super) fn browser_login_response(
    login_id: &str,
    auth_url: &str,
) -> Result<Value, ConnectionError> {
    Ok(json!({
        "type": "chatgpt",
        "loginId": login_id,
        "authUrl": auth_url,
    }))
}

fn device_login_response(login_id: &str) -> Result<Value, ConnectionError> {
    Ok(json!({
        "type": "chatgptDeviceCode",
        "loginId": login_id,
        "verificationUrl": "https://auth.openai.com/device",
        "userCode": "SAFE-CODE",
    }))
}

pub(super) fn browser_start_request() -> RecordedRequest {
    request(
        "account/login/start",
        json!({
            "type": "chatgpt",
            "useHostedLoginSuccessPage": true,
            "appBrand": "chatgpt",
        }),
    )
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

pub(super) fn browser_open_error() -> AccountStatus {
    AccountStatus::Error {
        message: "无法打开 ChatGPT 登录页面，请尝试设备码登录。".to_string(),
        retryable: true,
    }
}

pub(super) fn login_unavailable_error() -> AccountStatus {
    AccountStatus::Error {
        message: "ChatGPT 登录暂时不可用，请重试。".to_string(),
        retryable: true,
    }
}
