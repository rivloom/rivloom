use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::runtime_status::RuntimeStatus;

const INITIALIZE_REQUEST_ID: u64 = 0;
const CLIENT_NAME: &str = "rivloom_desktop";
const CLIENT_TITLE: &str = "Rivloom Desktop";

#[derive(Debug, Error)]
pub(crate) enum ProtocolError {
    #[error("failed to serialize an App Server protocol message")]
    SerializeJson(#[source] serde_json::Error),
    #[error("failed to parse an App Server protocol message")]
    InvalidJson(#[source] serde_json::Error),
    #[error("expected response ID {expected}, received {actual}")]
    UnexpectedResponseId { expected: u64, actual: u64 },
    #[error("App Server returned error {code}: {message}")]
    Remote { code: i64, message: String },
    #[error("App Server response contained neither one result nor one error")]
    InvalidResponse,
}

pub(crate) fn initialize_request() -> Result<String, ProtocolError> {
    json_line(&InitializeRequest {
        method: "initialize",
        id: INITIALIZE_REQUEST_ID,
        params: InitializeParams {
            client_info: ClientInfo {
                name: CLIENT_NAME,
                title: CLIENT_TITLE,
                version: env!("CARGO_PKG_VERSION"),
            },
        },
    })
}

pub(crate) fn initialized_notification() -> Result<String, ProtocolError> {
    json_line(&InitializedNotification {
        method: "initialized",
        params: EmptyParams {},
    })
}

pub(crate) fn parse_initialize_response(line: &str) -> Result<RuntimeStatus, ProtocolError> {
    let response: ResponseEnvelope<InitializeResult> =
        serde_json::from_str(line).map_err(ProtocolError::InvalidJson)?;

    if response.id != INITIALIZE_REQUEST_ID {
        return Err(ProtocolError::UnexpectedResponseId {
            expected: INITIALIZE_REQUEST_ID,
            actual: response.id,
        });
    }

    match (response.result, response.error) {
        (Some(result), None) => Ok(RuntimeStatus::Connected {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            app_server_user_agent: result.user_agent,
            platform: format!("{}/{}", result.platform_family, result.platform_os),
            codex_home: result.codex_home,
        }),
        (None, Some(error)) => Err(ProtocolError::Remote {
            code: error.code,
            message: error.message,
        }),
        _ => Err(ProtocolError::InvalidResponse),
    }
}

fn json_line(message: &impl Serialize) -> Result<String, ProtocolError> {
    let mut line = serde_json::to_string(message).map_err(ProtocolError::SerializeJson)?;
    line.push('\n');
    Ok(line)
}

#[derive(Serialize)]
struct InitializeRequest<'a> {
    method: &'static str,
    id: u64,
    params: InitializeParams<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitializeParams<'a> {
    client_info: ClientInfo<'a>,
}

#[derive(Serialize)]
struct ClientInfo<'a> {
    name: &'static str,
    title: &'static str,
    version: &'a str,
}

#[derive(Serialize)]
struct InitializedNotification {
    method: &'static str,
    params: EmptyParams,
}

#[derive(Serialize)]
struct EmptyParams {}

#[derive(Deserialize)]
struct ResponseEnvelope<T> {
    id: u64,
    result: Option<T>,
    error: Option<RemoteError>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializeResult {
    user_agent: String,
    codex_home: String,
    platform_family: String,
    platform_os: String,
}

#[derive(Deserialize)]
struct RemoteError {
    code: i64,
    message: String,
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
