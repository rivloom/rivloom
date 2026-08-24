use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RuntimeStatus {
    Starting,
    Connected {
        app_version: String,
        app_server_user_agent: String,
        platform: String,
        codex_home: String,
    },
    Error {
        message: String,
        retryable: bool,
    },
    Stopped,
}

#[cfg(test)]
#[path = "runtime_status_tests.rs"]
mod tests;
