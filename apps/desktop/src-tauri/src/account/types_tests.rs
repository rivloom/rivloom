use pretty_assertions::assert_eq;
use serde_json::json;

use super::CodexRuntimeAuthStatus;

#[test]
fn codex_runtime_auth_statuses_expose_only_the_frontend_contract() {
    let statuses = [
        CodexRuntimeAuthStatus::Checking,
        CodexRuntimeAuthStatus::SignedOut,
        CodexRuntimeAuthStatus::BrowserPending,
        CodexRuntimeAuthStatus::SignedIn {
            email: None,
            plan_type: "plus".to_string(),
        },
        CodexRuntimeAuthStatus::SignedIn {
            email: Some("user@example.com".to_string()),
            plan_type: "pro".to_string(),
        },
        CodexRuntimeAuthStatus::Error {
            message: "账号状态暂时不可用。".to_string(),
            retryable: true,
        },
    ];

    let actual = statuses
        .iter()
        .map(|status| serde_json::to_value(status).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        actual,
        vec![
            json!({ "state": "checking" }),
            json!({ "state": "signedOut" }),
            json!({ "state": "browserPending" }),
            json!({
                "state": "signedIn",
                "email": null,
                "planType": "plus",
            }),
            json!({
                "state": "signedIn",
                "email": "user@example.com",
                "planType": "pro",
            }),
            json!({
                "state": "error",
                "message": "账号状态暂时不可用。",
                "retryable": true,
            }),
        ]
    );
}
