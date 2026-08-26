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
use super::tests::FakeConnection;
use super::tests::FakeUrlOpener;
use super::tests::RecordedRequest;
use super::tests::request;
use crate::account::types::AccountStatus;
use crate::app_server::ConnectionControl;
use crate::app_server::ConnectionError;
use crate::app_server::NotificationObserver;

type BackgroundTask = Box<dyn FnOnce() + Send + 'static>;

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
    service.connect(Arc::new(ControlledConnection { request_sender }));
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
    service.connect(Arc::new(ControlledConnection { request_sender }));
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
    service.connect(Arc::new(ControlledConnection { request_sender }));
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

fn notify(service: &AccountService, method: &str, params: Value) {
    NotificationObserver::on_notification(service, method, &params);
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
