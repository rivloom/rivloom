use std::sync::Arc;
use std::sync::Mutex;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::AppServerConnection;
use super::ConnectionError;
use super::InboundMessage;
use super::NotificationObserver;

#[test]
fn notifications_are_forwarded_without_writing() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let writer_writes = writes.clone();
    let connection = AppServerConnection::new(move |line| {
        writer_writes.lock().unwrap().push(line.to_string());
        Ok(())
    });
    let observer = Arc::new(RecordingObserver::default());
    connection.set_notification_observer(observer.clone());

    connection
        .handle_inbound(InboundMessage::Notification {
            method: "account/updated".to_string(),
            params: json!({"authMode": "chatgpt"}),
        })
        .unwrap();

    assert_eq!(
        observer.notifications(),
        vec![(
            "account/updated".to_string(),
            json!({"authMode": "chatgpt"}),
        )]
    );
    assert_eq!(*writes.lock().unwrap(), Vec::<String>::new());
}

#[test]
fn server_requests_receive_method_not_supported() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let writer_writes = writes.clone();
    let connection = AppServerConnection::new(move |line| {
        writer_writes.lock().unwrap().push(line.to_string());
        Ok(())
    });

    connection
        .handle_inbound(InboundMessage::ServerRequest {
            id: json!("approval-1"),
            method: "item/commandExecution/requestApproval".to_string(),
            params: json!({"command": "TOP_SECRET"}),
        })
        .unwrap();

    assert_eq!(
        writes
            .lock()
            .unwrap()
            .iter()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>(),
        vec![json!({
            "id": "approval-1",
            "error": {
                "code": -32601,
                "message": "Method not supported",
            },
        })]
    );
}

#[test]
fn responses_without_pending_requests_are_ignored() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let writer_writes = writes.clone();
    let connection = AppServerConnection::new(move |line| {
        writer_writes.lock().unwrap().push(line.to_string());
        Ok(())
    });

    connection
        .handle_inbound(InboundMessage::Response {
            id: 7,
            result: json!({"ignored": true}),
        })
        .unwrap();
    connection
        .handle_inbound(InboundMessage::ResponseError {
            id: 8,
            code: -32600,
            message: "ignored".to_string(),
        })
        .unwrap();

    assert_eq!(*writes.lock().unwrap(), Vec::<String>::new());
}

#[test]
fn write_failures_are_sanitized() {
    let connection = AppServerConnection::new(|_| Err("TOP_SECRET".to_string()));

    let error = connection
        .handle_inbound(InboundMessage::ServerRequest {
            id: json!(1),
            method: "item/tool/call".to_string(),
            params: json!({"secret": "TOP_SECRET"}),
        })
        .unwrap_err();

    assert_eq!(error, ConnectionError::WriteFailed);
    assert!(!error.to_string().contains("TOP_SECRET"));
}

#[derive(Default)]
struct RecordingObserver {
    notifications: Mutex<Vec<(String, Value)>>,
}

impl RecordingObserver {
    fn notifications(&self) -> Vec<(String, Value)> {
        self.notifications.lock().unwrap().clone()
    }
}

impl NotificationObserver for RecordingObserver {
    fn on_notification(&self, method: &str, params: &Value) {
        self.notifications
            .lock()
            .unwrap()
            .push((method.to_string(), params.clone()));
    }
}
