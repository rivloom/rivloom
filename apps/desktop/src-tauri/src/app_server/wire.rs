use serde_json::Map;
use serde_json::Value;
use thiserror::Error;

pub(super) const MAX_JSON_LINE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum InboundMessage {
    Response {
        id: u64,
        result: Value,
    },
    ResponseError {
        id: u64,
        code: i64,
        message: String,
    },
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
    },
}

#[derive(Debug, Error)]
pub(super) enum WireError {
    #[error("failed to parse an App Server protocol message")]
    InvalidJson(#[source] serde_json::Error),
    #[error("App Server message must be a JSON object")]
    ExpectedObject,
    #[error("App Server request or notification must contain a string method")]
    InvalidMethod,
    #[error("App Server server-request ID must be a string or integer")]
    InvalidServerRequestId,
    #[error("App Server response ID must be a non-negative integer")]
    InvalidResponseId,
    #[error("App Server response must contain exactly one result or error")]
    InvalidResponseShape,
    #[error("App Server error response must contain an integer code and string message")]
    InvalidErrorShape,
    #[error("App Server emitted invalid UTF-8")]
    InvalidUtf8,
    #[error("App Server JSONL message exceeded the {max_bytes}-byte limit")]
    LineTooLarge { max_bytes: usize },
}

pub(super) fn parse_inbound_message(line: &str) -> Result<InboundMessage, WireError> {
    let value: Value = serde_json::from_str(line).map_err(WireError::InvalidJson)?;
    let object = value.as_object().ok_or(WireError::ExpectedObject)?;

    if object.contains_key("method") {
        parse_call(object)
    } else {
        parse_response(object)
    }
}

fn parse_call(object: &Map<String, Value>) -> Result<InboundMessage, WireError> {
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or(WireError::InvalidMethod)?
        .to_string();
    let params = object.get("params").cloned().unwrap_or(Value::Null);

    let Some(id) = object.get("id") else {
        return Ok(InboundMessage::Notification { method, params });
    };
    if !is_server_request_id(id) {
        return Err(WireError::InvalidServerRequestId);
    }

    Ok(InboundMessage::ServerRequest {
        id: id.clone(),
        method,
        params,
    })
}

fn is_server_request_id(id: &Value) -> bool {
    id.is_string() || id.as_i64().is_some() || id.as_u64().is_some()
}

fn parse_response(object: &Map<String, Value>) -> Result<InboundMessage, WireError> {
    let id = object
        .get("id")
        .and_then(Value::as_u64)
        .ok_or(WireError::InvalidResponseId)?;

    match (object.get("result"), object.get("error")) {
        (Some(result), None) => Ok(InboundMessage::Response {
            id,
            result: result.clone(),
        }),
        (None, Some(error)) => parse_error_response(id, error),
        _ => Err(WireError::InvalidResponseShape),
    }
}

fn parse_error_response(id: u64, error: &Value) -> Result<InboundMessage, WireError> {
    let error = error.as_object().ok_or(WireError::InvalidErrorShape)?;
    let code = error
        .get("code")
        .and_then(Value::as_i64)
        .ok_or(WireError::InvalidErrorShape)?;
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .ok_or(WireError::InvalidErrorShape)?
        .to_string();

    Ok(InboundMessage::ResponseError { id, code, message })
}

#[derive(Debug)]
pub(super) struct JsonLineDecoder {
    buffer: Vec<u8>,
    max_line_bytes: usize,
}

impl JsonLineDecoder {
    pub(super) fn new(max_line_bytes: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max_line_bytes,
        }
    }

    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, WireError> {
        let mut lines = Vec::new();

        for segment in chunk.split_inclusive(|byte| *byte == b'\n') {
            let has_newline = segment.last() == Some(&b'\n');
            let content = if has_newline {
                &segment[..segment.len() - 1]
            } else {
                segment
            };
            self.extend_buffer(content)?;

            if has_newline {
                lines.push(self.take_line()?);
            }
        }

        Ok(lines)
    }

    fn extend_buffer(&mut self, bytes: &[u8]) -> Result<(), WireError> {
        if bytes.len() > self.max_line_bytes.saturating_sub(self.buffer.len()) {
            self.buffer.clear();
            return Err(WireError::LineTooLarge {
                max_bytes: self.max_line_bytes,
            });
        }

        self.buffer.extend_from_slice(bytes);
        Ok(())
    }

    fn take_line(&mut self) -> Result<String, WireError> {
        if self.buffer.last() == Some(&b'\r') {
            self.buffer.pop();
        }

        String::from_utf8(std::mem::take(&mut self.buffer)).map_err(|_| WireError::InvalidUtf8)
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
