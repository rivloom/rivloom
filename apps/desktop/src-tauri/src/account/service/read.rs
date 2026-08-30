use serde::Deserialize;
use serde_json::Value;

use super::retryable_account_error;
use super::unsupported_account_error;
use crate::account::types::CodexRuntimeAuthStatus;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountReadResponse {
    account: Value,
    requires_openai_auth: bool,
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum AccountPayload {
    Chatgpt {
        email: Value,
        plan_type: String,
    },
    #[serde(other)]
    Unsupported,
}

pub(super) fn parse_account_status(result: Value) -> CodexRuntimeAuthStatus {
    let Ok(response) = serde_json::from_value::<AccountReadResponse>(result) else {
        return retryable_account_error();
    };

    if response.account.is_null() {
        return if response.requires_openai_auth {
            CodexRuntimeAuthStatus::SignedOut
        } else {
            unsupported_account_error()
        };
    }

    let Ok(account) = serde_json::from_value::<AccountPayload>(response.account) else {
        return retryable_account_error();
    };
    match account {
        AccountPayload::Chatgpt { email, plan_type } => {
            let email = match email {
                Value::Null => None,
                Value::String(email) => Some(email),
                _ => return retryable_account_error(),
            };
            CodexRuntimeAuthStatus::SignedIn { email, plan_type }
        }
        AccountPayload::Unsupported => unsupported_account_error(),
    }
}
