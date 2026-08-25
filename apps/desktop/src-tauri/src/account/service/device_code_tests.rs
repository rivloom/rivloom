use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::TryLockError;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::json;
use tauri::Url;

use super::AccountService;
use super::tests::FakeConnection;
use super::tests::RecordedRequest;
use super::tests::StatusTask;
use super::tests::browser_harness;
use super::tests::browser_login_response;
use super::tests::browser_start_request;
use super::tests::controlled_browser_service;
use super::tests::login_unavailable_error;
use super::tests::next_request;
use super::tests::request;
use super::tests::spawn_status_task;
use crate::account::login::UrlOpener;
use crate::account::types::AccountStatus;
use crate::app_server::ConnectionError;

#[test]
fn device_login_uses_fixed_params_and_exposes_only_approved_values() {
    let (service, connection, opener) = browser_harness(
        vec![device_login_response(
            "device-login",
            "https://auth.openai.com/codex/device",
            "ABCD-1234",
        )],
        vec![],
    );

    assert_eq!(service.start_device_code_login(), device_pending_status());
    assert_eq!(service.status(), device_pending_status());
    assert_eq!(connection.requests(), vec![device_start_request()]);
    assert_eq!(opener.opened_urls(), Vec::<String>::new());
}

#[test]
fn browser_and_device_starts_cancel_the_previous_attempt_before_switching() {
    let (service, connection, opener) = browser_harness(
        vec![
            device_login_response(
                "first-device",
                "https://auth.openai.com/codex/device",
                "FIRST-CODE",
            ),
            cancel_response(),
            browser_login_response("browser-login", "https://auth.openai.com/oauth"),
            cancel_response(),
            device_login_response(
                "second-device",
                "https://auth.openai.com/codex/device-two",
                "SECOND-CODE",
            ),
        ],
        vec![Ok(())],
    );

    assert_eq!(
        service.start_device_code_login(),
        AccountStatus::DevicePending {
            verification_url: "https://auth.openai.com/codex/device".to_string(),
            user_code: "FIRST-CODE".to_string(),
        }
    );
    assert_eq!(service.start_browser_login(), AccountStatus::BrowserPending);
    assert_eq!(
        service.start_device_code_login(),
        AccountStatus::DevicePending {
            verification_url: "https://auth.openai.com/codex/device-two".to_string(),
            user_code: "SECOND-CODE".to_string(),
        }
    );
    assert_eq!(
        connection.requests(),
        vec![
            device_start_request(),
            cancel_request("first-device"),
            browser_start_request(),
            cancel_request("browser-login"),
            device_start_request(),
        ]
    );
    assert_eq!(
        opener.opened_urls(),
        vec!["https://auth.openai.com/oauth".to_string()]
    );
}

#[test]
fn device_login_holds_the_serialization_gate_until_the_attempt_is_installed() {
    let (service, opener, request_receiver) = controlled_browser_service();
    let login = spawn_device_login(&service);
    let request = next_request(&request_receiver, "device login request should arrive");
    assert!(matches!(
        service.inner.login_operation.try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    request.respond(
        device_start_request(),
        device_login_response(
            "device-login",
            "https://auth.openai.com/codex/device",
            "ABCD-1234",
        ),
    );
    assert_eq!(
        (
            login.wait("device login should finish"),
            opener.opened_urls()
        ),
        (device_pending_status(), Vec::<String>::new())
    );
}

#[test]
fn a_valid_device_login_on_an_old_connection_is_canceled_without_exposing_values() {
    let (service, opener, request_receiver) = controlled_browser_service();
    let old_login = spawn_device_login(&service);
    let old_request = next_request(
        &request_receiver,
        "old device login should reach the connection",
    );
    service.connect(Arc::new(FakeConnection::new(vec![Ok(json!({
        "account": null,
        "requiresOpenaiAuth": true,
    }))])));
    assert_eq!(service.refresh(), AccountStatus::SignedOut);
    old_request.respond(
        device_start_request(),
        device_login_response(
            "old-device",
            "https://auth.openai.com/codex/device",
            "STALE-CODE",
        ),
    );
    next_request(&request_receiver, "old device login should be canceled")
        .respond(cancel_request("old-device"), cancel_response());

    assert_eq!(
        (
            old_login.wait("old device login cleanup should finish"),
            service.status(),
            opener.opened_urls(),
        ),
        (
            AccountStatus::SignedOut,
            AccountStatus::SignedOut,
            Vec::<String>::new(),
        )
    );
}

#[test]
fn verification_open_uses_the_stored_url_and_blocks_connection_transition() {
    let (entered_sender, entered_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let opener = Arc::new(BlockingUrlOpener {
        entered_sender: Mutex::new(Some(entered_sender)),
        release_receiver: Mutex::new(release_receiver),
    });
    let old_connection = Arc::new(FakeConnection::new(vec![device_login_response(
        "device-login",
        "https://auth.openai.com/codex/device",
        "ABCD-1234",
    )]));
    let service = AccountService::with_url_opener(opener);
    service.connect(old_connection);
    assert_eq!(service.start_device_code_login(), device_pending_status());

    let open_service = service.clone();
    let open_task = thread::spawn(move || open_service.open_device_verification());
    assert_eq!(
        entered_receiver
            .recv_timeout(Duration::from_secs(/*secs*/ 1))
            .expect("verification opener should receive the stored URL"),
        "https://auth.openai.com/codex/device".to_string()
    );
    assert!(matches!(
        service.inner.browser_open_operation.try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    assert!(matches!(
        service.inner.login_operation.try_lock(),
        Err(TryLockError::WouldBlock)
    ));

    let (connect_sender, connect_receiver) = mpsc::channel();
    let connect_service = service.clone();
    let connect_task = thread::spawn(move || {
        let status = connect_service.connect(Arc::new(FakeConnection::new(vec![])));
        connect_sender.send(status).unwrap();
    });

    release_sender.send(()).unwrap();
    assert_eq!(open_task.join().unwrap(), device_pending_status());
    assert_eq!(
        connect_receiver
            .recv_timeout(Duration::from_secs(/*secs*/ 1))
            .expect("connect should finish after verification opening"),
        AccountStatus::Checking
    );
    connect_task.join().unwrap();
}

#[test]
fn failed_verification_open_can_retry_and_restore_device_pending() {
    let (service, connection, opener) = browser_harness(
        vec![device_login_response(
            "device-login",
            "https://auth.openai.com/codex/device",
            "ABCD-1234",
        )],
        vec![Err(()), Ok(())],
    );
    assert_eq!(service.start_device_code_login(), device_pending_status());

    assert_eq!(
        service.open_device_verification(),
        device_verification_open_error()
    );
    assert_eq!(service.status(), device_verification_open_error());
    assert_eq!(service.open_device_verification(), device_pending_status());
    assert_eq!(service.status(), device_pending_status());
    assert_eq!(connection.requests(), vec![device_start_request()]);
    assert_eq!(
        opener.opened_urls(),
        vec![
            "https://auth.openai.com/codex/device".to_string(),
            "https://auth.openai.com/codex/device".to_string(),
        ]
    );
}

#[test]
fn switching_away_clears_device_values_and_prevents_reopening_them() {
    let (service, connection, opener) = browser_harness(
        vec![
            device_login_response(
                "device-login",
                "https://auth.openai.com/codex/device",
                "SECRET-CODE",
            ),
            cancel_response(),
            browser_login_response("browser-login", "https://auth.openai.com/oauth"),
        ],
        vec![Ok(())],
    );
    assert_eq!(
        service.start_device_code_login(),
        AccountStatus::DevicePending {
            verification_url: "https://auth.openai.com/codex/device".to_string(),
            user_code: "SECRET-CODE".to_string(),
        }
    );
    assert_eq!(service.start_browser_login(), AccountStatus::BrowserPending);

    assert_eq!(
        service.open_device_verification(),
        AccountStatus::BrowserPending
    );
    assert_eq!(
        connection.requests(),
        vec![
            device_start_request(),
            cancel_request("device-login"),
            browser_start_request(),
        ]
    );
    assert_eq!(
        opener.opened_urls(),
        vec!["https://auth.openai.com/oauth".to_string()]
    );
}

#[test]
fn invalid_or_mismatched_device_starts_are_canceled_without_exposing_values() {
    let (service, connection, opener) = browser_harness(
        vec![
            browser_login_response("browser-mismatch", "https://auth.openai.com/oauth"),
            cancel_response(),
            device_login_response(
                "unsafe-device",
                "https://openai.com.attacker.example/device",
                "SECRET-ONE",
            ),
            cancel_response(),
            device_login_response("empty-code", "https://auth.openai.com/codex/device", ""),
            cancel_response(),
        ],
        vec![],
    );

    assert_eq!(service.start_device_code_login(), login_unavailable_error());
    assert_eq!(service.start_device_code_login(), login_unavailable_error());
    assert_eq!(service.start_device_code_login(), login_unavailable_error());
    assert_eq!(service.status(), login_unavailable_error());
    assert_eq!(
        connection.requests(),
        vec![
            device_start_request(),
            cancel_request("browser-mismatch"),
            device_start_request(),
            cancel_request("unsafe-device"),
            device_start_request(),
            cancel_request("empty-code"),
        ]
    );
    assert_eq!(opener.opened_urls(), Vec::<String>::new());
}

#[test]
fn failed_rejected_device_cleanup_retains_only_a_non_reopenable_cleanup_handle() {
    let (service, connection, opener) = browser_harness(
        vec![
            device_login_response(
                "rejected-device",
                "https://auth.openai.com/codex/device",
                "",
            ),
            Err(ConnectionError::Disconnected),
            cancel_response(),
            device_login_response(
                "replacement-device",
                "https://auth.openai.com/codex/device",
                "ABCD-1234",
            ),
        ],
        vec![],
    );

    assert_eq!(service.start_device_code_login(), login_unavailable_error());
    assert_eq!(
        service.open_device_verification(),
        login_unavailable_error()
    );
    assert_eq!(opener.opened_urls(), Vec::<String>::new());
    assert_eq!(service.start_device_code_login(), device_pending_status());
    assert_eq!(
        connection.requests(),
        vec![
            device_start_request(),
            cancel_request("rejected-device"),
            cancel_request("rejected-device"),
            device_start_request(),
        ]
    );
    assert_eq!(opener.opened_urls(), Vec::<String>::new());
}

struct BlockingUrlOpener {
    entered_sender: Mutex<Option<mpsc::Sender<String>>>,
    release_receiver: Mutex<mpsc::Receiver<()>>,
}

impl UrlOpener for BlockingUrlOpener {
    fn open(&self, url: &Url) -> Result<(), ()> {
        self.entered_sender
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .expect("opener should be called once")
            .send(url.as_str().to_string())
            .unwrap();
        self.release_receiver
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv_timeout(Duration::from_secs(/*secs*/ 2))
            .map_err(|_| ())
    }
}

fn spawn_device_login(service: &AccountService) -> StatusTask {
    let service = service.clone();
    spawn_status_task(move || service.start_device_code_login())
}

fn device_login_response(
    login_id: &str,
    verification_url: &str,
    user_code: &str,
) -> Result<serde_json::Value, ConnectionError> {
    Ok(json!({
        "type": "chatgptDeviceCode",
        "loginId": login_id,
        "verificationUrl": verification_url,
        "userCode": user_code,
    }))
}

fn cancel_response() -> Result<serde_json::Value, ConnectionError> {
    Ok(json!({ "status": "canceled" }))
}

fn device_start_request() -> RecordedRequest {
    request(
        "account/login/start",
        json!({ "type": "chatgptDeviceCode" }),
    )
}

fn cancel_request(login_id: &str) -> RecordedRequest {
    request("account/login/cancel", json!({ "loginId": login_id }))
}

fn device_pending_status() -> AccountStatus {
    AccountStatus::DevicePending {
        verification_url: "https://auth.openai.com/codex/device".to_string(),
        user_code: "ABCD-1234".to_string(),
    }
}

fn device_verification_open_error() -> AccountStatus {
    AccountStatus::Error {
        message: "无法打开设备码验证页面，请手动打开该地址后重试。".to_string(),
        retryable: true,
    }
}
