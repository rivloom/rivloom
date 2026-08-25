use std::fmt;

use serde::Deserialize;
use serde_json::Value;
use tauri::Url;

/// Opens only URLs that the account service has already parsed and approved.
///
/// Implementations must not log the URL because it can contain temporary OAuth data.
pub(crate) trait UrlOpener: Send + Sync {
    fn open(&self, url: &Url) -> Result<(), ()>;
}

#[derive(Deserialize, Eq, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum LoginStartResponse {
    Chatgpt {
        login_id: String,
        auth_url: String,
    },
    ChatgptDeviceCode {
        login_id: String,
        verification_url: String,
        user_code: String,
    },
    #[serde(other)]
    Unsupported,
}

impl fmt::Debug for LoginStartResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Chatgpt { .. } => formatter.write_str("Chatgpt { .. }"),
            Self::ChatgptDeviceCode { .. } => formatter.write_str("ChatgptDeviceCode { .. }"),
            Self::Unsupported => formatter.write_str("Unsupported"),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelLoginResponse {
    status: CancelLoginStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum CancelLoginStatus {
    Canceled,
    NotFound,
}

pub(super) fn parse_login_response(result: Value) -> Option<LoginStartResponse> {
    serde_json::from_value(result).ok()
}

pub(super) fn is_cancel_confirmation(result: Value) -> bool {
    let Ok(response) = serde_json::from_value::<CancelLoginResponse>(result) else {
        return false;
    };
    match response.status {
        CancelLoginStatus::Canceled | CancelLoginStatus::NotFound => true,
    }
}

pub(super) fn parse_official_auth_url(raw_url: &str) -> Option<Url> {
    let url = Url::parse(raw_url).ok()?;
    let host = url.host_str()?;
    let official_host = ["chatgpt.com", "openai.com"]
        .into_iter()
        .any(|root| host == root || host.ends_with(&format!(".{root}")));
    if url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && official_host
    {
        Some(url)
    } else {
        None
    }
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
