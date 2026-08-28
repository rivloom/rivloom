use std::sync::{Arc, TryLockError};

use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::AccountService;
use super::retryable_account_error;
use super::tests::FakeConnection;
use super::tests::FakeUrlOpener;
use super::tests::RecordedRequest;
use super::tests::browser_login_response;
use super::tests::browser_start_request;
use super::tests::controlled_browser_service;
use super::tests::login_unavailable_error;
use super::tests::next_request;
use super::tests::request;
use super::tests::request_without_params;
use super::tests::signed_in_response;
use super::tests::spawn_status_task;
use crate::account::types::AccountStatus;
use crate::app_server::ConnectionError;

#[test]
fn explicit_cancel_cleans_the_attempt_and_rereads_backend_truth() {
    let (service, connection) = harness(vec![
        browser_login_response("cancel-me", "https://auth.openai.com/oauth"),
        cancel_response(),
        signed_out_response(),
    ]);
    assert_eq!(service.start_browser_login(), AccountStatus::BrowserPending);

    assert_eq!(service.cancel_account_login(), AccountStatus::SignedOut);
    assert_eq!(
        (service.status(), connection.requests()),
        (
            AccountStatus::SignedOut,
            vec![
                browser_start_request(),
                cancel_request("cancel-me"),
                account_read_request(),
            ]
        )
    );
}

#[test]
fn failed_cancel_keeps_a_cleanup_handle_for_a_later_retry() {
    let (service, connection) = harness(vec![
        browser_login_response("retry-cancel", "https://auth.openai.com/oauth"),
        Err(ConnectionError::Timeout),
        cancel_response(),
        signed_out_response(),
    ]);
    assert_eq!(service.start_browser_login(), AccountStatus::BrowserPending);

    assert_eq!(service.cancel_account_login(), login_unavailable_error());
    assert_eq!(service.cancel_account_login(), AccountStatus::SignedOut);
    assert_eq!(
        connection.requests(),
        vec![
            browser_start_request(),
            cancel_request("retry-cancel"),
            cancel_request("retry-cancel"),
            account_read_request(),
        ]
    );
}

#[test]
fn successful_logout_is_parameterless_and_rereads_backend_truth() {
    let (service, connection) = harness(vec![
        signed_in_response(),
        Ok(json!({})),
        signed_out_response(),
    ]);
    assert_eq!(service.refresh(), signed_in_status());

    assert_eq!(service.logout_account(), AccountStatus::SignedOut);
    assert_eq!(
        connection.requests(),
        vec![
            account_read_request(),
            request_without_params("account/logout"),
            account_read_request(),
        ]
    );
}

#[test]
fn logout_failures_preserve_the_signed_in_state() {
    for logout_response in [
        Err(ConnectionError::Timeout),
        Ok(json!({ "unexpected": true })),
    ] {
        let (service, _connection) = harness(vec![signed_in_response(), logout_response]);
        assert_eq!(service.refresh(), signed_in_status());

        assert_eq!(
            (service.logout_account(), service.status()),
            (logout_error(), signed_in_status())
        );
    }
}

#[test]
fn logout_after_canceling_a_pending_login_never_retains_temporary_values() {
    let (service, connection) = harness(vec![
        browser_login_response("cancel-before-logout", "https://auth.openai.com/oauth"),
        cancel_response(),
        Err(ConnectionError::Timeout),
    ]);
    assert_eq!(service.start_browser_login(), AccountStatus::BrowserPending);

    assert_eq!(
        (service.logout_account(), service.status()),
        (logout_error(), AccountStatus::Checking)
    );
    assert_eq!(
        connection.requests(),
        vec![
            browser_start_request(),
            cancel_request("cancel-before-logout"),
            request_without_params("account/logout"),
        ]
    );
}

#[test]
fn stale_logout_failures_return_the_reconnected_account_status() {
    let (service, _, request_receiver) = controlled_browser_service();
    let initial_read = {
        let service = service.clone();
        spawn_status_task(move || service.refresh())
    };
    next_request(&request_receiver, "initial account/read should arrive")
        .respond(account_read_request(), signed_in_response());
    assert_eq!(
        initial_read.wait("initial account/read should finish"),
        signed_in_status()
    );

    let logout = {
        let service = service.clone();
        spawn_status_task(move || service.logout_account())
    };
    let old_logout = next_request(&request_receiver, "old account/logout should arrive");
    let current_connection = Arc::new(FakeConnection::new(vec![signed_out_response()]));
    assert_eq!(service.connect(current_connection), AccountStatus::Checking);
    assert_eq!(service.refresh(), AccountStatus::SignedOut);
    old_logout.respond(
        request_without_params("account/logout"),
        Err(ConnectionError::Timeout),
    );

    assert_eq!(
        (logout.wait("stale logout should finish"), service.status()),
        (AccountStatus::SignedOut, AccountStatus::SignedOut)
    );
}

#[test]
fn logout_holds_the_login_gate_through_its_follow_up_read() {
    let (service, _, request_receiver) = controlled_browser_service();
    let logout = {
        let service = service.clone();
        spawn_status_task(move || service.logout_account())
    };
    let logout_request = next_request(&request_receiver, "account/logout should arrive");
    assert!(matches!(
        service.inner.login_operation.try_lock(),
        Err(TryLockError::WouldBlock)
    ));

    let login = {
        let service = service.clone();
        spawn_status_task(move || service.start_browser_login())
    };
    logout_request.respond(request_without_params("account/logout"), Ok(json!({})));
    next_request(
        &request_receiver,
        "logout follow-up account/read should arrive before login",
    )
    .respond(account_read_request(), signed_out_response());
    assert_eq!(
        logout.wait("logout should finish"),
        AccountStatus::SignedOut
    );
    next_request(&request_receiver, "browser login should run after logout").respond(
        browser_start_request(),
        browser_login_response("after-logout", "https://auth.openai.com/oauth"),
    );

    assert_eq!(
        login.wait("browser login should finish"),
        AccountStatus::BrowserPending
    );
}

#[test]
fn confirmed_cancel_invalidates_an_older_account_read() {
    let (service, _, request_receiver) = controlled_browser_service();
    let login = {
        let service = service.clone();
        spawn_status_task(move || service.start_browser_login())
    };
    next_request(&request_receiver, "browser login should start").respond(
        browser_start_request(),
        browser_login_response("cancel-with-stale-read", "https://auth.openai.com/oauth"),
    );
    assert_eq!(
        login.wait("browser login should finish"),
        AccountStatus::BrowserPending
    );

    let stale_read = {
        let service = service.clone();
        spawn_status_task(move || service.refresh())
    };
    let stale_request = next_request(&request_receiver, "stale account/read should arrive");
    let logout = {
        let service = service.clone();
        spawn_status_task(move || service.logout_account())
    };
    next_request(&request_receiver, "active login should be canceled")
        .respond(cancel_request("cancel-with-stale-read"), cancel_response());
    next_request(&request_receiver, "logout should follow cancellation").respond(
        request_without_params("account/logout"),
        Err(ConnectionError::Timeout),
    );
    assert_eq!(
        (logout.wait("logout should fail"), service.status()),
        (logout_error(), AccountStatus::Checking)
    );

    stale_request.respond(account_read_request(), signed_in_response());
    assert_eq!(
        (
            stale_read.wait("stale account/read should finish"),
            service.status()
        ),
        (AccountStatus::Checking, AccountStatus::Checking)
    );
}

#[test]
fn successful_cancel_with_failed_reread_clears_temporary_values() {
    let (service, _connection) = harness(vec![
        browser_login_response("clear-on-reread-failure", "https://auth.openai.com/oauth"),
        cancel_response(),
        Err(ConnectionError::Timeout),
    ]);
    assert_eq!(service.start_browser_login(), AccountStatus::BrowserPending);

    assert_eq!(
        (service.cancel_account_login(), service.status()),
        (retryable_account_error(), retryable_account_error())
    );
    assert!(
        !serde_json::to_string(&service.status())
            .unwrap()
            .contains("ABCD-1234")
    );
}

#[test]
fn successful_logout_with_failed_reread_does_not_retain_signed_in() {
    let (service, _connection) = harness(vec![
        signed_in_response(),
        Ok(json!({})),
        Err(ConnectionError::Timeout),
    ]);
    assert_eq!(service.refresh(), signed_in_status());

    assert_eq!(
        (service.logout_account(), service.status()),
        (retryable_account_error(), retryable_account_error())
    );
}

fn harness(
    responses: Vec<Result<Value, ConnectionError>>,
) -> (AccountService, Arc<FakeConnection>) {
    let connection = Arc::new(FakeConnection::new(responses));
    let service = AccountService::with_url_opener(Arc::new(FakeUrlOpener::new(vec![])));
    service.connect(connection.clone());
    (service, connection)
}

fn account_read_request() -> RecordedRequest {
    request("account/read", json!({ "refreshToken": false }))
}

pub(super) fn cancel_request(login_id: &str) -> RecordedRequest {
    request("account/login/cancel", json!({ "loginId": login_id }))
}

pub(super) fn cancel_response() -> Result<Value, ConnectionError> {
    Ok(json!({ "status": "canceled" }))
}

pub(super) fn signed_out_response() -> Result<Value, ConnectionError> {
    Ok(json!({ "account": null, "requiresOpenaiAuth": true }))
}

fn signed_in_status() -> AccountStatus {
    AccountStatus::SignedIn {
        email: None,
        plan_type: "plus".to_string(),
    }
}

fn logout_error() -> AccountStatus {
    AccountStatus::Error {
        message: "无法退出 ChatGPT，请重试。".to_string(),
        retryable: true,
    }
}
