use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::mpsc::Sender;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use crate::app_server::wire::InboundMessage;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(/*secs*/ 10);
const FIRST_REQUEST_ID: u64 = 1;
const METHOD_NOT_SUPPORTED_CODE: i64 = -32601;
const METHOD_NOT_SUPPORTED_MESSAGE: &str = "Method not supported";
const MAX_PENDING_REQUESTS: usize = 64;

/// Sends bounded App Server requests for higher-level Rust services.
///
/// Implementations must correlate each response with its caller and return only
/// sanitized errors that are safe to pass into service-level state mapping.
pub(crate) trait ConnectionControl: Send + Sync {
    fn request(&self, method: &str, params: Value) -> Result<Value, ConnectionError>;

    fn request_without_params(&self, method: &str) -> Result<Value, ConnectionError>;
}

/// Receives App Server notifications inside the Rust backend.
///
/// Implementations should return promptly and must decide which normalized
/// state, if any, is safe to expose outside the backend.
pub(crate) trait NotificationObserver: Send + Sync {
    fn on_notification(&self, method: &str, params: &Value);
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub(crate) enum ConnectionError {
    #[error("failed to serialize an App Server protocol message")]
    Serialize,
    #[error("failed to send a request to App Server")]
    WriteFailed,
    #[error("App Server request timed out")]
    Timeout,
    #[error("too many App Server requests are already pending")]
    TooManyPending,
    #[error("App Server connection is closed")]
    Disconnected,
    #[error("App Server request failed with code {code}")]
    Remote { code: i64 },
    #[error("App Server request IDs are exhausted")]
    RequestIdExhausted,
}

type PendingResult = Result<Value, ConnectionError>;
type MessageWriter = dyn Fn(&str) -> Result<(), String> + Send + Sync;

enum RequestParameters<'a> {
    Present(&'a Value),
    Omitted,
}

#[derive(Clone)]
pub(super) struct AppServerConnection {
    inner: Arc<ConnectionInner>,
}

struct ConnectionInner {
    writer: Box<MessageWriter>,
    next_request_id: AtomicU64,
    state: Mutex<ConnectionState>,
    observer: Mutex<Option<Arc<dyn NotificationObserver>>>,
    options: ConnectionOptions,
}

struct ConnectionState {
    connected: bool,
    pending: HashMap<u64, Sender<PendingResult>>,
}

#[derive(Clone, Copy)]
struct ConnectionOptions {
    request_timeout: Duration,
    max_pending_requests: usize,
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            max_pending_requests: MAX_PENDING_REQUESTS,
        }
    }
}

impl AppServerConnection {
    pub(super) fn new(writer: impl Fn(&str) -> Result<(), String> + Send + Sync + 'static) -> Self {
        Self::with_options(writer, ConnectionOptions::default())
    }

    fn with_options(
        writer: impl Fn(&str) -> Result<(), String> + Send + Sync + 'static,
        options: ConnectionOptions,
    ) -> Self {
        Self {
            inner: Arc::new(ConnectionInner {
                writer: Box::new(writer),
                next_request_id: AtomicU64::new(FIRST_REQUEST_ID),
                state: Mutex::new(ConnectionState {
                    connected: true,
                    pending: HashMap::new(),
                }),
                observer: Mutex::new(None),
                options,
            }),
        }
    }

    pub(super) fn set_notification_observer(&self, observer: Arc<dyn NotificationObserver>) {
        *self
            .inner
            .observer
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(observer);
    }

    pub(super) fn handle_inbound(&self, message: InboundMessage) -> Result<(), ConnectionError> {
        match message {
            InboundMessage::Response { id, result } => {
                self.complete(id, Ok(result));
            }
            InboundMessage::ResponseError { id, code, .. } => {
                self.complete(id, Err(ConnectionError::Remote { code }));
            }
            InboundMessage::Notification { method, params } => {
                let observer = self
                    .inner
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
                (self.inner.writer.as_ref())(&line).map_err(|_| ConnectionError::WriteFailed)?;
            }
        }

        Ok(())
    }

    pub(super) fn disconnect(&self) {
        let waiters = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if !state.connected {
                return;
            }
            state.connected = false;
            state
                .pending
                .drain()
                .map(|(_, sender)| sender)
                .collect::<Vec<_>>()
        };

        for waiter in waiters {
            let _ = waiter.send(Err(ConnectionError::Disconnected));
        }
    }

    fn complete(&self, id: u64, result: PendingResult) {
        let waiter = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pending
            .remove(&id);
        if let Some(waiter) = waiter {
            let _ = waiter.send(result);
        }
    }

    fn remove_pending(&self, id: u64) -> Option<Sender<PendingResult>> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pending
            .remove(&id)
    }

    fn request_inner(
        &self,
        method: &str,
        params: RequestParameters<'_>,
    ) -> Result<Value, ConnectionError> {
        let (waiter, response) = mpsc::channel();
        let id = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if !state.connected {
                return Err(ConnectionError::Disconnected);
            }
            if state.pending.len() >= self.inner.options.max_pending_requests {
                return Err(ConnectionError::TooManyPending);
            }

            let id = self
                .inner
                .next_request_id
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
                    id.checked_add(/*rhs*/ 1)
                })
                .map_err(|_| ConnectionError::RequestIdExhausted)?;
            state.pending.insert(id, waiter);
            id
        };

        let params = match params {
            RequestParameters::Present(params) => Some(params),
            RequestParameters::Omitted => None,
        };
        let line = match json_line(&Request { method, id, params }) {
            Ok(line) => line,
            Err(error) => {
                self.remove_pending(id);
                return Err(error);
            }
        };
        if (self.inner.writer.as_ref())(&line).is_err() {
            self.remove_pending(id);
            return Err(ConnectionError::WriteFailed);
        }

        match response.recv_timeout(self.inner.options.request_timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => {
                if self.remove_pending(id).is_some() {
                    Err(ConnectionError::Timeout)
                } else {
                    response
                        .recv()
                        .unwrap_or(Err(ConnectionError::Disconnected))
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.remove_pending(id);
                Err(ConnectionError::Disconnected)
            }
        }
    }
}

impl ConnectionControl for AppServerConnection {
    fn request(&self, method: &str, params: Value) -> Result<Value, ConnectionError> {
        self.request_inner(method, RequestParameters::Present(&params))
    }

    fn request_without_params(&self, method: &str) -> Result<Value, ConnectionError> {
        self.request_inner(method, RequestParameters::Omitted)
    }
}

fn json_line(message: &impl Serialize) -> Result<String, ConnectionError> {
    let mut line = serde_json::to_string(message).map_err(|_| ConnectionError::Serialize)?;
    line.push('\n');
    Ok(line)
}

#[derive(Serialize)]
struct Request<'a> {
    method: &'a str,
    id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<&'a Value>,
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
mod inbound_tests;

#[cfg(test)]
#[path = "connection_request_tests.rs"]
mod request_tests;
