use pretty_assertions::assert_eq;
use serde_json::json;

use super::AccountStatus;

#[test]
fn account_statuses_expose_only_the_frontend_contract() {
    let statuses = [
        AccountStatus::Checking,
        AccountStatus::SignedOut,
        AccountStatus::BrowserPending,
        AccountStatus::DevicePending {
            verification_url: "https://auth.openai.com/codex/device".to_string(),
            user_code: "ABCD-1234".to_string(),
        },
        AccountStatus::SignedIn {
            email: None,
            plan_type: "plus".to_string(),
        },
        AccountStatus::Error {
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
                "state": "devicePending",
                "verificationUrl": "https://auth.openai.com/codex/device",
                "userCode": "ABCD-1234",
            }),
            json!({
                "state": "signedIn",
                "email": null,
                "planType": "plus",
            }),
            json!({
                "state": "error",
                "message": "账号状态暂时不可用。",
                "retryable": true,
            }),
        ]
    );
}
