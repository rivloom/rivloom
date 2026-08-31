use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::credential::CredentialBinding;
use super::protocol::{Message, id};
use super::reconcile::{MAX_CONTROL_BYTES, Page, ReconcileRequest};
use super::secret_store::SecretField;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    pub(super) version: u32,
    pub(super) id: String,
    pub(super) operation: Operation,
}

#[derive(Deserialize, Serialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum Operation {
    Authenticate {
        binding: CredentialBinding,
        secret: SecretField,
    },
    Join {
        brain_id: String,
        invitation_id: String,
        secret: SecretField,
        identity_id: String,
        device_id: String,
        display_name: String,
    },
    Sync(ReconcileRequest),
    Submit(Box<Message>),
    Pulse {},
    Invite {},
    CancelInvite {
        invitation_id: String,
    },
    Revoke {
        member_id: String,
        revision: u64,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Response {
    pub(super) version: u32,
    pub(super) id: String,
    pub(super) result: Reply,
}

#[derive(Deserialize, Serialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum Reply {
    Authenticated {},
    Joined {
        binding: CredentialBinding,
        expires_at: i64,
        secret: SecretField,
    },
    Page(Box<Page>),
    Applied {
        key: String,
        revision: u64,
    },
    Pulsed {
        revision: u64,
    },
    Invited {
        brain_id: String,
        invitation_id: String,
        expires_at: i64,
        secret: SecretField,
    },
    Administered {
        revision: u64,
    },
    Error(WireError),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum WireError {
    Rejected,
    Conflict,
    Busy,
    Unavailable,
    Invalid,
}

impl Request {
    pub(super) fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        if bytes.len() > MAX_CONTROL_BYTES {
            return Err(WireError::Invalid);
        }
        let request: Self = serde_json::from_slice(bytes).map_err(|_| WireError::Invalid)?;
        if request.version != 1 || !id(&request.id) {
            return Err(WireError::Invalid);
        }
        Ok(request)
    }
    pub(super) fn encode(&self) -> Result<Zeroizing<Vec<u8>>, WireError> {
        let bytes = Zeroizing::new(serde_json::to_vec(self).map_err(|_| WireError::Invalid)?);
        Self::decode(&bytes)?;
        Ok(bytes)
    }
}
impl Response {
    pub(super) fn decode(bytes: &[u8], request_id: &str) -> Result<Self, WireError> {
        if bytes.len() > MAX_CONTROL_BYTES {
            return Err(WireError::Invalid);
        }
        let response: Self = serde_json::from_slice(bytes).map_err(|_| WireError::Invalid)?;
        if response.version != 1 || response.id != request_id || !id(&response.id) {
            return Err(WireError::Invalid);
        }
        if let Reply::Page(page) = &response.result {
            page.validate().map_err(|_| WireError::Invalid)?;
        }
        Ok(response)
    }
    pub(super) fn encode(&self) -> Result<Zeroizing<Vec<u8>>, WireError> {
        let bytes = Zeroizing::new(serde_json::to_vec(self).map_err(|_| WireError::Invalid)?);
        Self::decode(&bytes, &self.id)?;
        Ok(bytes)
    }
}
