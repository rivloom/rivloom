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
use super::TaskSpawner;
use super::login_completion::MAX_EARLY_COMPLETION_BYTES;
use super::login_completion::MAX_EARLY_COMPLETION_IDS;
use super::tests::FakeConnection;
use super::tests::FakeUrlOpener;
use super::tests::RecordedRequest;
use super::tests::browser_login_response;
use super::tests::browser_start_request;
use super::tests::request;
use crate::account::types::AccountStatus;
use crate::app_server::ConnectionControl;
use crate::app_server::ConnectionError;
use crate::app_server::ConnectionIdentity;
use crate::app_server::NotificationObserver;

type BackgroundTask = Box<dyn FnOnce() + Send + 'static>;

#[test]
fn matching_completion_before_browser_attempt_install_skips_temporary_state() {
    let (service, connection, opener, tasks) = early_completion_harness(
        vec![
            browser_login_response("early-browser", "https://auth.openai.com/oauth"),
            signed_in_response(),
        ],
        vec!["early-browser".to_string()],
    );

    assert_eq!(service.start_browser_login(), AccountStatus::Checking);
    assert_eq!(
        (
            service.status(),
            opener.opened_urls(),
            tasks.len(),
            connection.requests(),
        ),
        (
            AccountStatus::Checking,
            vec![],
            1,
            vec![browser_start_request()],
        )
    );

    tasks.run_next();
    assert_eq!(
        (service.status(), connection.requests()),
        (
            signed_in_status(),
            vec![browser_start_request(), account_read_request()],
        )
    );
}

#[test]
fn matching_completion_before_device_attempt_install_skips_temporary_state() {
    let (service, connection, opener, tasks) = early_completion_harness(
        vec![device_login_response("early-device"), signed_in_response()],
        vec!["early-device".to_string()],
    );

    assert_eq!(service.start_device_code_login(), AccountStatus::Checking);
    assert_eq!(
        (
            service.status(),
            opener.opened_urls(),
            tasks.len(),
            connection.requests(),
        ),
        (
            AccountStatus::Checking,
            vec![],
            1,
            vec![device_start_request()],
        )
    );

    tasks.run_next();
    assert_eq!(
        (service.status(), connection.requests()),
        (
            signed_in_status(),
            vec![device_start_request(), account_read_request()],
        )
    );
}

#[test]
fn completion_from_previous_connection_cannot_finish_a_new_login_start() {
    let tasks = Arc::new(ManualTaskSpawner::default());
    let service =
        AccountService::with_dependencies(Arc::new(FakeUrlOpener::new(vec![])), tasks.clone());
    let previous_connection = Arc::new(FakeConnection::new(vec![]));
    let previous_identity = previous_connection.connection_identity();
    service.connect(previous_connection);

    let (request_sender, request_receiver) = mpsc::channel();
    service.connect(Arc::new(ControlledConnection {
        identity: ConnectionIdentity::new(),
        request_sender,
    }));
    let login_service = service.clone();
    let login = thread::spawn(move || login_service.start_device_code_login());
    let start = next_request(&request_receiver);

    notify_from(
        &service,
        &previous_identity,
        "account/login/completed",
        json!({ "loginId": "reused-login", "success": true }),
    );
    start.respond(device_login_response("reused-login"));

    assert_eq!(
        (login.join().unwrap(), service.status(), tasks.len()),
        (device_pending_status(), device_pending_status(), 0)
    );
}

#[test]
fn account_update_from_previous_connection_does_not_schedule_a_refresh() {
    let tasks = Arc::new(ManualTaskSpawner::default());
    let service =
        AccountService::with_dependencies(Arc::new(FakeUrlOpener::new(vec![])), tasks.clone());
    let previous_connection = Arc::new(FakeConnection::new(vec![]));
    let previous_identity = previous_connection.connection_identity();
    service.connect(previous_connection);
    service.connect(Arc::new(FakeConnection::new(vec![])));

    notify_from(
        &service,
        &previous_identity,
        "account/updated",
        json!({ "authMode": "chatgpt" }),
    );

    assert_eq!(
        (service.status(), tasks.len()),
        (AccountStatus::Checking, 0)
    );
}

#[test]
fn failed_completion_before_attempt_install_skips_temporary_state_and_rereads() {
    let tasks = Arc::new(ManualTaskSpawner::default());
    let service =
        AccountService::with_dependencies(Arc::new(FakeUrlOpener::new(vec![])), tasks.clone());
    let (request_sender, request_receiver) = mpsc::channel();
    service.connect(Arc::new(ControlledConnection {
        identity: ConnectionIdentity::new(),
        request_sender,
    }));
    let login_service = service.clone();
    let login = thread::spawn(move || login_service.start_device_code_login());
    let start = next_request(&request_receiver);

    notify(
        &service,
        "account/login/completed",
        json!({
            "loginId": "early-failure",
            "success": false,
            "error": "TOP_SECRET",
        }),
    );
    start.respond(device_login_response("early-failure"));

    assert_eq!(
        (login.join().unwrap(), service.status(), tasks.len()),
        (AccountStatus::Checking, AccountStatus::Checking, 1)
    );
    let refresh = thread::spawn(tasks.take_next());
    next_request(&request_receiver).respond(signed_out_response());
    refresh.join().unwrap();
    assert_eq!(service.status(), AccountStatus::SignedOut);
    assert!(
        !serde_json::to_string(&service.status())
            .unwrap()
            .contains("TOP_SECRET")
    );
}

#[test]
fn empty_oversized_and_duplicate_early_ids_do_not_exhaust_the_window() {
    let mut completion_ids = vec![String::new(), "x".repeat(MAX_EARLY_COMPLETION_BYTES + 1)];
    completion_ids.extend(std::iter::repeat_n(
        "duplicate-stale".to_string(),
        MAX_EARLY_COMPLETION_IDS + 1,
    ));
    completion_ids.push("bounded-login".to_string());
    let (service, connection, _opener, tasks) = early_completion_harness(
        vec![device_login_response("bounded-login"), signed_in_response()],
        completion_ids,
    );

    assert_eq!(service.start_device_code_login(), AccountStatus::Checking);
    assert_eq!(
        (tasks.len(), connection.requests()),
        (1, vec![device_start_request()])
    );
    tasks.run_next();
    assert_eq!(
        (service.status(), connection.requests()),
        (
            signed_in_status(),
            vec![device_start_request(), account_read_request()],
        )
    );
}

#[test]
fn unique_early_ids_evict_the_oldest_and_keep_the_latest_completion() {
    let mut completion_ids = (0..MAX_EARLY_COMPLETION_IDS)
        .map(|index| format!("stale-{index}"))
        .collect::<Vec<_>>();
    completion_ids.push("capacity-login".to_string());
    let (service, connection, _opener, tasks) = early_completion_harness(
        vec![
            device_login_response("capacity-login"),
            signed_in_response(),
        ],
        completion_ids,
    );

    assert_eq!(service.start_device_code_login(), AccountStatus::Checking);
    assert_eq!(
        (tasks.len(), connection.requests()),
        (1, vec![device_start_request()])
    );
    tasks.run_next();
    assert_eq!(service.status(), signed_in_status());
}

#[test]
fn unique_early_id_capacity_evicts_the_oldest_completion() {
    let mut completion_ids = vec!["oldest-login".to_string()];
    completion_ids
        .extend((0..MAX_EARLY_COMPLETION_IDS).map(|index| format!("newer-stale-{index}")));
    let (service, connection, _opener, tasks) =
        early_completion_harness(vec![device_login_response("oldest-login")], completion_ids);

    assert_eq!(service.start_device_code_login(), device_pending_status());
    assert_eq!(
        (tasks.len(), connection.requests()),
        (0, vec![device_start_request()])
    );
}

#[test]
fn duplicate_early_id_refreshes_fifo_position_without_consuming_capacity() {
    let anchor_id = "capacity-anchor";
    let duplicate_id = "repeated-login";
    let mut completion_ids = full_early_id_window(anchor_id, duplicate_id);
    completion_ids.push(duplicate_id.to_string());
    let (service, _connection, _opener, tasks) = early_completion_harness(
        vec![device_login_response(anchor_id), signed_in_response()],
        completion_ids,
    );
    assert_eq!(service.start_device_code_login(), AccountStatus::Checking);
    assert_eq!(tasks.len(), 1);
    tasks.run_next();
    assert_eq!(service.status(), signed_in_status());

    let mut completion_ids = full_early_id_window(anchor_id, duplicate_id);
    completion_ids.extend([
        duplicate_id.to_string(),
        "newest-stale-one".to_string(),
        "newest-stale-two".to_string(),
    ]);
    let (service, _connection, _opener, tasks) = early_completion_harness(
        vec![device_login_response(duplicate_id), signed_in_response()],
        completion_ids,
    );
    assert_eq!(service.start_device_code_login(), AccountStatus::Checking);
    assert_eq!(tasks.len(), 1);
    tasks.run_next();
    assert_eq!(service.status(), signed_in_status());
}

#[test]
fn aggregate_early_id_bytes_are_capped_at_the_configured_limit() {
    let matching_id = "m".repeat(MAX_EARLY_COMPLETION_BYTES / 2);
    let exact_filler = "e".repeat(MAX_EARLY_COMPLETION_BYTES - matching_id.len());
    let (service, _connection, _opener, tasks) = early_completion_harness(
        vec![device_login_response(&matching_id), signed_in_response()],
        vec![matching_id.clone(), exact_filler],
    );
    assert_eq!(service.start_device_code_login(), AccountStatus::Checking);
    assert_eq!(tasks.len(), 1);
    tasks.run_next();
    assert_eq!(service.status(), signed_in_status());

    let overflow_filler = "o".repeat(MAX_EARLY_COMPLETION_BYTES - matching_id.len() + 1);
    let (service, connection, _opener, tasks) = early_completion_harness(
        vec![device_login_response(&matching_id)],
        vec![matching_id, overflow_filler],
    );
    assert_eq!(service.start_device_code_login(), device_pending_status());
    assert_eq!(
        (tasks.len(), connection.requests()),
        (0, vec![device_start_request()])
    );
}

fn full_early_id_window(anchor_id: &str, duplicate_id: &str) -> Vec<String> {
    let mut completion_ids = vec![anchor_id.to_string(), duplicate_id.to_string()];
    completion_ids
        .extend((0..MAX_EARLY_COMPLETION_IDS - 2).map(|index| format!("window-stale-{index}")));
    completion_ids
}

#[test]
fn matching_success_and_failure_completions_clear_temporary_values_then_refresh() {
    for success in [true, false] {
        let (service, connection, tasks) = harness(vec![
            device_login_response("matching-login"),
            signed_in_response(),
        ]);
        assert_eq!(service.start_device_code_login(), device_pending_status());

        let params = if success {
            json!({ "loginId": "matching-login", "success": true })
        } else {
            json!({
                "loginId": "matching-login",
                "success": false,
                "error": "TOP_SECRET",
            })
        };
        notify(&service, "account/login/completed", params);
        assert_eq!(
            (service.status(), tasks.len(), connection.requests()),
            (AccountStatus::Checking, 1, vec![device_start_request()],)
        );

        tasks.run_next();
        assert_eq!(
            (service.status(), connection.requests()),
            (
                signed_in_status(),
                vec![device_start_request(), account_read_request()],
            )
        );
        assert!(
            !serde_json::to_string(&service.status())
                .unwrap()
                .contains("TOP_SECRET")
        );
    }
}

#[test]
fn stale_and_malformed_completion_notifications_are_ignored() {
    let (service, connection, tasks) = harness(vec![device_login_response("current-login")]);
    assert_eq!(service.start_device_code_login(), device_pending_status());

    for params in [
        json!({ "loginId": "stale-login", "success": true, "error": null }),
        json!({ "loginId": null, "success": true, "error": null }),
        json!({ "loginId": 42, "success": true, "error": null }),
        json!({ "success": true, "error": null }),
        json!({ "loginId": "current-login" }),
        json!({ "loginId": "current-login", "success": "yes", "error": null }),
        json!({ "loginId": "current-login", "success": true, "error": {} }),
    ] {
        notify(&service, "account/login/completed", params);
    }
    notify(
        &service,
        "unrelated/event",
        json!({ "loginId": "current-login" }),
    );

    assert_eq!(
        (service.status(), tasks.len(), connection.requests()),
        (device_pending_status(), 0, vec![device_start_request()])
    );
}

#[test]
fn duplicate_notifications_before_task_start_coalesce_into_one_read() {
    let (service, connection, tasks) = harness(vec![signed_out_response()]);

    notify(&service, "account/updated", json!({}));
    notify(&service, "account/updated", json!({}));
    assert_eq!((tasks.len(), connection.requests()), (1, vec![]));

    tasks.run_next();
    assert_eq!(
        (service.status(), tasks.len(), connection.requests()),
        (AccountStatus::SignedOut, 0, vec![account_read_request()],)
    );
}

#[test]
fn notifications_during_a_read_schedule_exactly_one_follow_up() {
    let tasks = Arc::new(ManualTaskSpawner::default());
    let service =
        AccountService::with_dependencies(Arc::new(FakeUrlOpener::new(vec![])), tasks.clone());
    let (request_sender, request_receiver) = mpsc::channel();
    service.connect(Arc::new(ControlledConnection {
        identity: ConnectionIdentity::new(),
        request_sender,
    }));
    notify(&service, "account/updated", json!({}));

    let first_task = thread::spawn(tasks.take_next());
    let first_read = next_request(&request_receiver);
    assert_eq!(first_read.request, account_read_request());
    notify(&service, "account/updated", json!({}));
    notify(&service, "account/updated", json!({}));
    assert_eq!(tasks.len(), 0);
    first_read.respond(signed_out_response());
    first_task.join().unwrap();
    assert_eq!(
        (service.status(), tasks.len()),
        (AccountStatus::SignedOut, 1)
    );

    let second_task = thread::spawn(tasks.take_next());
    let second_read = next_request(&request_receiver);
    assert_eq!(second_read.request, account_read_request());
    second_read.respond(signed_in_response());
    second_task.join().unwrap();
    assert_eq!((service.status(), tasks.len()), (signed_in_status(), 0));
}

#[test]
fn reconnect_invalidates_queued_and_running_refreshes_from_old_connections() {
    let (service, old_connection, tasks) = harness(vec![signed_in_response()]);
    notify(&service, "account/updated", json!({}));
    assert_eq!(tasks.len(), 1);

    let current_connection = Arc::new(FakeConnection::new(vec![signed_out_response()]));
    assert_eq!(
        service.connect(current_connection.clone()),
        AccountStatus::Checking
    );
    tasks.run_next();
    assert_eq!(
        (
            service.status(),
            old_connection.requests(),
            current_connection.requests(),
        ),
        (AccountStatus::Checking, vec![], vec![])
    );

    notify(&service, "account/updated", json!({}));
    tasks.run_next();
    assert_eq!(
        (service.status(), current_connection.requests()),
        (AccountStatus::SignedOut, vec![account_read_request()])
    );

    let (request_sender, request_receiver) = mpsc::channel();
    service.connect(Arc::new(ControlledConnection {
        identity: ConnectionIdentity::new(),
        request_sender,
    }));
    notify(&service, "account/updated", json!({}));
    let old_task = thread::spawn(tasks.take_next());
    let old_read = next_request(&request_receiver);
    let newest_connection = Arc::new(FakeConnection::new(vec![signed_out_response()]));
    service.connect(newest_connection.clone());
    notify(&service, "account/updated", json!({}));
    old_read.respond(signed_in_response());
    old_task.join().unwrap();
    assert_eq!(
        (service.status(), tasks.len(), newest_connection.requests()),
        (AccountStatus::Checking, 1, vec![])
    );
    tasks.run_next();
    assert_eq!(
        (service.status(), newest_connection.requests()),
        (AccountStatus::SignedOut, vec![account_read_request()])
    );
}

#[test]
fn a_failed_task_spawn_does_not_block_the_next_notification() {
    let (service, connection, tasks) = harness(vec![signed_out_response()]);
    tasks.fail_next();
    notify(&service, "account/updated", json!({}));
    assert_eq!((tasks.len(), connection.requests()), (0, vec![]));

    notify(&service, "account/updated", json!({}));
    assert_eq!(tasks.len(), 1);
    tasks.run_next();
    assert_eq!(
        (service.status(), connection.requests()),
        (AccountStatus::SignedOut, vec![account_read_request()])
    );
}

#[test]
fn completion_during_failed_cancel_wins_over_the_stale_action_error() {
    let tasks = Arc::new(ManualTaskSpawner::default());
    let service =
        AccountService::with_dependencies(Arc::new(FakeUrlOpener::new(vec![])), tasks.clone());
    let (request_sender, request_receiver) = mpsc::channel();
    service.connect(Arc::new(ControlledConnection {
        identity: ConnectionIdentity::new(),
        request_sender,
    }));
    let login_service = service.clone();
    let login = thread::spawn(move || login_service.start_device_code_login());
    next_request(&request_receiver).respond(device_login_response("complete-during-cancel"));
    assert_eq!(login.join().unwrap(), device_pending_status());

    let cancel_service = service.clone();
    let cancel = thread::spawn(move || cancel_service.cancel_account_login());
    let cancel_request = next_request(&request_receiver);
    assert_eq!(
        cancel_request.request,
        request(
            "account/login/cancel",
            json!({ "loginId": "complete-during-cancel" }),
        )
    );
    notify(
        &service,
        "account/login/completed",
        json!({
            "loginId": "complete-during-cancel",
            "success": false,
            "error": "TOP_SECRET",
        }),
    );
    cancel_request.respond(Err(ConnectionError::Timeout));

    assert_eq!(
        (cancel.join().unwrap(), service.status(), tasks.len()),
        (AccountStatus::Checking, AccountStatus::Checking, 1)
    );
    let refresh = thread::spawn(tasks.take_next());
    next_request(&request_receiver).respond(signed_out_response());
    refresh.join().unwrap();
    assert_eq!(service.status(), AccountStatus::SignedOut);
}

fn harness(
    responses: Vec<Result<Value, ConnectionError>>,
) -> (AccountService, Arc<FakeConnection>, Arc<ManualTaskSpawner>) {
    let connection = Arc::new(FakeConnection::new(responses));
    let tasks = Arc::new(ManualTaskSpawner::default());
    let service =
        AccountService::with_dependencies(Arc::new(FakeUrlOpener::new(vec![])), tasks.clone());
    service.connect(connection.clone());
    (service, connection, tasks)
}

fn early_completion_harness(
    responses: Vec<Result<Value, ConnectionError>>,
    completion_ids: Vec<String>,
) -> (
    AccountService,
    Arc<EarlyCompletionConnection>,
    Arc<FakeUrlOpener>,
    Arc<ManualTaskSpawner>,
) {
    let connection = Arc::new(EarlyCompletionConnection::new(responses, completion_ids));
    let opener = Arc::new(FakeUrlOpener::new(vec![]));
    let tasks = Arc::new(ManualTaskSpawner::default());
    let service = AccountService::with_dependencies(opener.clone(), tasks.clone());
    connection.observe(service.clone());
    service.connect(connection.clone());
    (service, connection, opener, tasks)
}

fn notify(service: &AccountService, method: &str, params: Value) {
    let connection_identity = service
        .inner
        .state
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .connection_identity
        .clone()
        .expect("the account service should be connected");
    notify_from(service, &connection_identity, method, params);
}

fn notify_from(
    service: &AccountService,
    connection_identity: &ConnectionIdentity,
    method: &str,
    params: Value,
) {
    NotificationObserver::on_notification(service, connection_identity, method, &params);
}

#[derive(Default)]
struct ManualTaskSpawner {
    fail_next: Mutex<bool>,
    tasks: Mutex<VecDeque<BackgroundTask>>,
}

impl ManualTaskSpawner {
    fn fail_next(&self) {
        *self
            .fail_next
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = true;
    }

    fn len(&self) -> usize {
        self.tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    fn take_next(&self) -> BackgroundTask {
        self.tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .expect("a background task should be queued")
    }

    fn run_next(&self) {
        self.take_next()();
    }
}

impl TaskSpawner for ManualTaskSpawner {
    fn spawn(&self, task: BackgroundTask) -> Result<(), ()> {
        if std::mem::take(
            &mut *self
                .fail_next
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        ) {
            return Err(());
        }
        self.tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(task);
        Ok(())
    }
}

fn next_request(receiver: &mpsc::Receiver<ControlledRequest>) -> ControlledRequest {
    receiver
        .recv_timeout(Duration::from_secs(/*secs*/ 1))
        .expect("the expected App Server request should arrive")
}

struct ControlledRequest {
    request: RecordedRequest,
    response_sender: mpsc::Sender<Result<Value, ConnectionError>>,
}

impl ControlledRequest {
    fn respond(self, response: Result<Value, ConnectionError>) {
        self.response_sender.send(response).unwrap();
    }
}

struct ControlledConnection {
    identity: ConnectionIdentity,
    request_sender: mpsc::Sender<ControlledRequest>,
}

struct EarlyCompletionConnection {
    completion_ids: Vec<String>,
    identity: ConnectionIdentity,
    observer: Mutex<Option<AccountService>>,
    requests: Mutex<Vec<RecordedRequest>>,
    responses: Mutex<VecDeque<Result<Value, ConnectionError>>>,
}

impl EarlyCompletionConnection {
    fn new(responses: Vec<Result<Value, ConnectionError>>, completion_ids: Vec<String>) -> Self {
        Self {
            completion_ids,
            identity: ConnectionIdentity::new(),
            observer: Mutex::new(None),
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(responses.into()),
        }
    }

    fn observe(&self, service: AccountService) {
        *self.observer.lock().unwrap_or_else(PoisonError::into_inner) = Some(service);
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl ConnectionControl for EarlyCompletionConnection {
    fn connection_identity(&self) -> ConnectionIdentity {
        self.identity.clone()
    }

    fn request(&self, method: &str, params: Value) -> Result<Value, ConnectionError> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request(method, params));
        let response = self
            .responses
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or(Err(ConnectionError::Disconnected));
        if method == "account/login/start" {
            let observer = self
                .observer
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
                .expect("the account service should observe start completion");
            for login_id in &self.completion_ids {
                notify_from(
                    &observer,
                    &self.identity,
                    "account/login/completed",
                    json!({ "loginId": login_id, "success": true }),
                );
            }
        }
        response
    }

    fn request_without_params(&self, method: &str) -> Result<Value, ConnectionError> {
        self.request(method, Value::Null)
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
        self.request(method, Value::Null)
    }
}

fn account_read_request() -> RecordedRequest {
    request("account/read", json!({ "refreshToken": false }))
}

fn device_start_request() -> RecordedRequest {
    request(
        "account/login/start",
        json!({ "type": "chatgptDeviceCode" }),
    )
}

fn device_login_response(login_id: &str) -> Result<Value, ConnectionError> {
    Ok(json!({
        "type": "chatgptDeviceCode",
        "loginId": login_id,
        "verificationUrl": "https://auth.openai.com/codex/device",
        "userCode": "ABCD-1234",
    }))
}

fn signed_in_response() -> Result<Value, ConnectionError> {
    Ok(json!({
        "account": { "type": "chatgpt", "email": null, "planType": "plus" },
        "requiresOpenaiAuth": true,
    }))
}

fn signed_out_response() -> Result<Value, ConnectionError> {
    Ok(json!({ "account": null, "requiresOpenaiAuth": true }))
}

fn signed_in_status() -> AccountStatus {
    AccountStatus::SignedIn {
        email: None,
        plan_type: "plus".to_string(),
    }
}

fn device_pending_status() -> AccountStatus {
    AccountStatus::DevicePending {
        verification_url: "https://auth.openai.com/codex/device".to_string(),
        user_code: "ABCD-1234".to_string(),
    }
}
