use std::sync::Arc;

use pretty_assertions::assert_eq;
use serde_json::json;

use super::AccountCommand;
use super::AccountService;
use super::account_actions_tests::cancel_request;
use super::account_actions_tests::cancel_response;
use super::account_actions_tests::device_login_response;
use super::account_actions_tests::device_start_request;
use super::account_actions_tests::signed_out_response;
use super::retryable_account_error;
use super::tests::FakeConnection;
use super::tests::FakeUrlOpener;
use super::tests::RecordedRequest;
use super::tests::browser_login_response;
use super::tests::browser_start_request;
use super::tests::request;
use super::tests::request_without_params;

#[test]
fn fixed_commands_drive_repeated_login_cancel_and_logout_requests() {
    let connection = Arc::new(FakeConnection::new(vec![
        signed_out_response(),
        browser_login_response("browser-login-1", "https://auth.openai.com/oauth"),
        cancel_response(),
        browser_login_response("browser-login-2", "https://auth.openai.com/oauth"),
        cancel_response(),
        signed_out_response(),
        signed_out_response(),
        device_login_response("device-login"),
        cancel_response(),
        Ok(json!({})),
        signed_out_response(),
        Ok(json!({})),
        signed_out_response(),
    ]));
    let opener = Arc::new(FakeUrlOpener::new(vec![Ok(()), Ok(()), Ok(())]));
    let service = AccountService::with_url_opener(opener.clone());
    service.connect(connection.clone());

    for command in [
        AccountCommand::GetStatus,
        AccountCommand::StartChatgptLogin,
        AccountCommand::StartChatgptLogin,
        AccountCommand::CancelLogin,
        AccountCommand::CancelLogin,
        AccountCommand::StartDeviceCodeLogin,
        AccountCommand::OpenDeviceVerification,
        AccountCommand::Logout,
        AccountCommand::Logout,
    ] {
        let _ = service.execute_command(command);
    }

    assert_eq!(
        connection.requests(),
        vec![
            account_read_request(),
            browser_start_request(),
            cancel_request("browser-login-1"),
            browser_start_request(),
            cancel_request("browser-login-2"),
            account_read_request(),
            account_read_request(),
            device_start_request(),
            cancel_request("device-login"),
            request_without_params("account/logout"),
            account_read_request(),
            request_without_params("account/logout"),
            account_read_request(),
        ]
    );
    assert_eq!(
        opener.opened_urls(),
        vec![
            "https://auth.openai.com/oauth".to_string(),
            "https://auth.openai.com/oauth".to_string(),
            "https://auth.openai.com/codex/device".to_string(),
        ]
    );
}

#[test]
fn every_fixed_command_returns_the_same_safe_error_when_disconnected() {
    let service = AccountService::new();

    assert_eq!(
        [
            AccountCommand::GetStatus,
            AccountCommand::StartChatgptLogin,
            AccountCommand::StartDeviceCodeLogin,
            AccountCommand::CancelLogin,
            AccountCommand::Logout,
            AccountCommand::OpenDeviceVerification,
        ]
        .map(|command| service.execute_command(command)),
        std::array::from_fn(|_| retryable_account_error())
    );
}

fn account_read_request() -> RecordedRequest {
    request("account/read", json!({ "refreshToken": false }))
}
