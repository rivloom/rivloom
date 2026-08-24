use serde_json::json;

use super::{
    ProtocolError, initialize_request, initialized_notification, parse_initialize_response,
};
use crate::runtime_status::RuntimeStatus;

#[test]
fn initialize_request_uses_the_reserved_id_and_rivloom_client_info() {
    let line = initialize_request().unwrap();

    assert!(line.ends_with('\n'));
    assert_eq!(line.lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(line.trim_end()).unwrap(),
        json!({
            "method": "initialize",
            "id": 0,
            "params": {
                "clientInfo": {
                    "name": "rivloom_desktop",
                    "title": "Rivloom Desktop",
                    "version": "0.1.0-alpha.0",
                },
            },
        })
    );
}

#[test]
fn initialized_notification_is_exactly_one_jsonl_message() {
    let line = initialized_notification().unwrap();

    assert_eq!(line, "{\"method\":\"initialized\",\"params\":{}}\n");
    assert_eq!(line.lines().count(), 1);
}

#[test]
fn successful_response_becomes_connected_runtime_status() {
    let response = r#"{
        "id": 0,
        "result": {
            "userAgent": "codex-app-server/1.2.3",
            "codexHome": "C:\\Users\\demo\\Rivloom\\codex-home",
            "platformFamily": "windows",
            "platformOs": "windows"
        }
    }"#;

    assert_eq!(
        parse_initialize_response(response).unwrap(),
        RuntimeStatus::Connected {
            app_version: "0.1.0-alpha.0".to_string(),
            app_server_user_agent: "codex-app-server/1.2.3".to_string(),
            platform: "windows/windows".to_string(),
            codex_home: r"C:\Users\demo\Rivloom\codex-home".to_string(),
        }
    );
}

#[test]
fn error_response_becomes_a_typed_protocol_error() {
    let error = parse_initialize_response(
        r#"{"id":0,"error":{"code":-32000,"message":"Not initialized"}}"#,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ProtocolError::Remote {
            code: -32000,
            ref message,
        } if message == "Not initialized"
    ));
}

#[test]
fn invalid_json_becomes_a_parse_error_without_panicking() {
    let error = parse_initialize_response("{not-json").unwrap_err();

    assert!(matches!(error, ProtocolError::InvalidJson(_)));
}

#[test]
fn response_with_the_wrong_id_is_rejected() {
    let error = parse_initialize_response(
        r#"{
            "id": 7,
            "result": {
                "userAgent": "codex-app-server/1.2.3",
                "codexHome": "C:\\Users\\demo\\Rivloom\\codex-home",
                "platformFamily": "windows",
                "platformOs": "windows"
            }
        }"#,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ProtocolError::UnexpectedResponseId {
            expected: 0,
            actual: 7,
        }
    ));
}
