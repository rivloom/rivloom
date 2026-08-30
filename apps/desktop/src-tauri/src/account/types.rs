use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum CodexRuntimeAuthStatus {
    Checking,
    SignedOut,
    BrowserPending,
    SignedIn {
        email: Option<String>,
        plan_type: String,
    },
    Error {
        message: String,
        retryable: bool,
    },
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
