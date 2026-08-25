use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use crate::app_server::wire::InboundMessage;

const METHOD_NOT_SUPPORTED_CODE: i64 = -32601;
const METHOD_NOT_SUPPORTED_MESSAGE: &str = "Method not supported";

/// Receives App Server notifications inside the Rust backend.
///
/// Implementations should return promptly and must decide which normalized
/// state, if any, is safe to expose outside the backend.
pub(super) trait NotificationObserver: Send + Sync {
    fn on_notification(&self, method: &str, params: &Value);
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub(super) enum ConnectionError {
    #[error("failed to serialize an App Server protocol message")]
    Serialize,
    #[error("failed to send a request to App Server")]
    WriteFailed,
}

type MessageWriter = dyn Fn(&str) -> Result<(), String> + Send + Sync;

pub(super) struct AppServerConnection {
    writer: Box<MessageWriter>,
    observer: Mutex<Option<Arc<dyn NotificationObserver>>>,
}

impl AppServerConnection {
    pub(super) fn new(writer: impl Fn(&str) -> Result<(), String> + Send + Sync + 'static) -> Self {
        Self {
            writer: Box::new(writer),
            observer: Mutex::new(None),
        }
    }

    pub(super) fn set_notification_observer(&self, observer: Arc<dyn NotificationObserver>) {
        *self.observer.lock().unwrap_or_else(PoisonError::into_inner) = Some(observer);
    }

    pub(super) fn handle_inbound(&self, message: InboundMessage) -> Result<(), ConnectionError> {
        match message {
            InboundMessage::Response { .. } | InboundMessage::ResponseError { .. } => {}
            InboundMessage::Notification { method, params } => {
                let observer = self
                    .observer
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                if let Some(observer) = observer {
                    observer.on_notification(&method, &params);
                }
            }
            InboundMessage::ServerRequest { id, .. } => {
                let line = json_line(&ErrorResponse {
                    id: &id,
                    error: ResponseError {
                        code: METHOD_NOT_SUPPORTED_CODE,
                        message: METHOD_NOT_SUPPORTED_MESSAGE,
                    },
                })?;
                (self.writer.as_ref())(&line).map_err(|_| ConnectionError::WriteFailed)?;
            }
        }

        Ok(())
    }
}

fn json_line(message: &impl Serialize) -> Result<String, ConnectionError> {
    let mut line = serde_json::to_string(message).map_err(|_| ConnectionError::Serialize)?;
    line.push('\n');
    Ok(line)
}

#[derive(Serialize)]
struct ErrorResponse<'a> {
    id: &'a Value,
    error: ResponseError,
}

#[derive(Serialize)]
struct ResponseError {
    code: i64,
    message: &'static str,
}

#[cfg(test)]
#[path = "connection_inbound_tests.rs"]
mod tests;
