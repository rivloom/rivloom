use pretty_assertions::assert_eq;
use serde_json::json;

use super::LoginStartResponse;
use super::is_cancel_confirmation;
use super::parse_login_response;
use super::parse_official_auth_url;

#[test]
fn login_start_responses_are_parsed_without_exposing_unknown_payloads() {
    assert_eq!(
        [
            parse_login_response(json!({
                "type": "chatgpt",
                "loginId": "browser-login",
                "authUrl": "https://auth.openai.com/oauth",
            })),
            parse_login_response(json!({
                "type": "chatgptDeviceCode",
                "loginId": "device-login",
                "verificationUrl": "https://auth.openai.com/device",
                "userCode": "TEST-CODE",
            })),
            parse_login_response(json!({
                "type": "futureLogin",
                "secret": "must-not-be-retained",
            })),
            parse_login_response(json!({ "type": "chatgpt", "loginId": 7 })),
        ],
        [
            Some(LoginStartResponse::Chatgpt {
                login_id: "browser-login".to_string(),
                auth_url: "https://auth.openai.com/oauth".to_string(),
            }),
            Some(LoginStartResponse::ChatgptDeviceCode {
                login_id: "device-login".to_string(),
                verification_url: "https://auth.openai.com/device".to_string(),
                user_code: "TEST-CODE".to_string(),
            }),
            Some(LoginStartResponse::Unsupported),
            None,
        ]
    );
}

#[test]
fn login_response_debug_output_is_redacted() {
    let response = parse_login_response(json!({
        "type": "chatgpt",
        "loginId": "TOP_SECRET_LOGIN_ID",
        "authUrl": "https://auth.openai.com/oauth?secret=TOP_SECRET",
    }))
    .unwrap();

    assert_eq!(format!("{response:?}"), "Chatgpt { .. }");
}

#[test]
fn cancel_confirmation_accepts_only_known_terminal_statuses() {
    assert_eq!(
        [
            json!({ "status": "canceled" }),
            json!({ "status": "notFound" }),
            json!({ "status": "futureStatus" }),
            json!({ "status": 7, "secret": "must-not-be-retained" }),
        ]
        .map(is_cancel_confirmation),
        [true, true, false, false]
    );
}

#[test]
fn official_auth_url_validation_rejects_unsafe_lookalikes() {
    assert_eq!(
        [
            "https://chatgpt.com/auth",
            "https://auth.openai.com/oauth",
            "https://login.eu.chatgpt.com/device",
            "https://openai.com:443/auth",
        ]
        .map(|raw_url| parse_official_auth_url(raw_url).is_some()),
        [true; 4]
    );
    assert_eq!(
        [
            "http://auth.openai.com/oauth",
            "https://openai.com.evil.example/oauth",
            "https://chatgpt.com.evil.example/device",
            "https://user@openai.com/oauth",
            "https://openai.com:8443/oauth",
            "not-a-url",
        ]
        .map(|raw_url| parse_official_auth_url(raw_url).is_some()),
        [false; 6]
    );
}
