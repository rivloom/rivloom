use serde_json::json;

use super::RuntimeStatus;

#[test]
fn starting_and_stopped_statuses_only_expose_the_state() {
    assert_eq!(
        serde_json::to_value(RuntimeStatus::Starting).unwrap(),
        json!({ "state": "starting" })
    );
    assert_eq!(
        serde_json::to_value(RuntimeStatus::Stopped).unwrap(),
        json!({ "state": "stopped" })
    );
}

#[test]
fn connected_status_matches_the_frontend_contract() {
    let status = RuntimeStatus::Connected {
        app_version: "0.1.0-alpha.0".to_string(),
        app_server_user_agent: "codex-app-server/1.2.3".to_string(),
        platform: "windows/x86_64".to_string(),
        codex_home: r"C:\Users\demo\Rivloom\codex-home".to_string(),
    };

    assert_eq!(
        serde_json::to_value(status).unwrap(),
        json!({
            "state": "connected",
            "appVersion": "0.1.0-alpha.0",
            "appServerUserAgent": "codex-app-server/1.2.3",
            "platform": "windows/x86_64",
            "codexHome": r"C:\Users\demo\Rivloom\codex-home",
        })
    );
}

#[test]
fn error_status_matches_the_frontend_contract() {
    let status = RuntimeStatus::Error {
        message: "核心服务暂时无法启动。".to_string(),
        retryable: true,
    };

    assert_eq!(
        serde_json::to_value(status).unwrap(),
        json!({
            "state": "error",
            "message": "核心服务暂时无法启动。",
            "retryable": true,
        })
    );
}
