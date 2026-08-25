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
use super::tests::browser_harness;
use super::tests::browser_login_response;
use super::tests::browser_start_request;
use super::tests::login_unavailable_error;
use super::tests::request;
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
