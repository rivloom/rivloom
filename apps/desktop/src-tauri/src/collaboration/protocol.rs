use serde::{Deserialize, Serialize};

const MAX_MESSAGE_BYTES: usize = 32 * 1024;
const MAX_REVISION: u64 = 9_007_199_254_740_991;

// Only Message crosses the boundary; Envelope is an unvalidated staging value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(try_from = "Envelope")]
pub(super) struct Message(Envelope);

impl Message {
    pub(super) fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(ProtocolError::MessageTooLarge);
        }
        serde_json::from_slice(bytes).map_err(|_| ProtocolError::InvalidMessage)
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        let bytes = serde_json::to_vec(&self.0).map_err(|_| ProtocolError::InvalidMessage)?;
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(ProtocolError::MessageTooLarge);
        }
        Ok(bytes)
    }
}

impl TryFrom<Envelope> for Message {
    type Error = ProtocolError;

    fn try_from(value: Envelope) -> Result<Self, Self::Error> {
        if value.protocol_version != 1
            || ![
                &value.message_id,
                &value.idempotency_key,
                &value.brain_id,
                &value.sender_node_id,
            ]
            .into_iter()
            .all(|value| id(value))
            || !timestamp(value.sent_at)
            || value.revision > MAX_REVISION
            || !value.payload.valid()
        {
            return Err(ProtocolError::InvalidMessage);
        }
        let message = Self(value);
        message.encode()?;
        Ok(message)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    protocol_version: u32,
    message_id: String,
    idempotency_key: String,
    brain_id: String,
    sender_node_id: String,
    sent_at: i64,
    revision: u64,
    payload: Payload,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "camelCase",
    deny_unknown_fields
)]
enum Payload {
    Identity(Identity),
    Node(Node),
    Task(Task),
    Assignment(Assignment),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Identity {
    identity_id: String,
    member_id: String,
    device_id: String,
    display_name: String,
    role: Role,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Role {
    Owner,
    Member,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Node {
    node_id: String,
    member_id: String,
    device_id: String,
    runtime_id: RuntimeId,
    runtime_version: String,
    capabilities: Vec<Capability>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum RuntimeId {
    Codex,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Capability {
    TaskRun,
    Interrupt,
    Patch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Task {
    task_id: String,
    created_by_member_id: String,
    goal: String,
    constraints: Vec<String>,
    expected_artifact: ArtifactKind,
    status: TaskStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum ArtifactKind {
    Patch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum TaskStatus {
    Draft,
    Offered,
    Accepted,
    Running,
    AwaitingReview,
    Approved,
    Rejected,
    Cancelled,
    Failed,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Assignment {
    assignment_id: String,
    task_id: String,
    offered_by_member_id: String,
    target_node_id: String,
    execution_policy: ExecutionPolicy,
    decision: Decision,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum ExecutionPolicy {
    ManagedWorktreeOffline,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Decision {
    Offered {},
    Accepted {
        accepted_by_member_id: String,
        project_ref: String,
        run_id: String,
        run_key: String,
        accepted_at: i64,
    },
    Rejected {
        decided_by_member_id: String,
        decided_at: i64,
    },
    Cancelled {
        decided_by_member_id: String,
        decided_at: i64,
    },
}

impl Payload {
    fn valid(&self) -> bool {
        match self {
            Self::Identity(value) => {
                [&value.identity_id, &value.member_id, &value.device_id]
                    .into_iter()
                    .all(|v| id(v))
                    && text(&value.display_name, 80)
            }
            Self::Node(value) => {
                [&value.node_id, &value.member_id, &value.device_id]
                    .into_iter()
                    .all(|v| id(v))
                    && text(&value.runtime_version, 128)
                    && value.capabilities.len() <= 3
                    && value
                        .capabilities
                        .iter()
                        .enumerate()
                        .all(|(i, capability)| !value.capabilities[..i].contains(capability))
            }
            Self::Task(value) => {
                id(&value.task_id)
                    && id(&value.created_by_member_id)
                    && text(&value.goal, 4096)
                    && value.constraints.len() <= 32
                    && value.constraints.iter().all(|v| text(v, 1024))
                    && value.constraints.iter().map(String::len).sum::<usize>() <= 8192
            }
            Self::Assignment(value) => {
                let decision_valid = match &value.decision {
                    Decision::Offered {} => true,
                    Decision::Accepted {
                        accepted_by_member_id,
                        project_ref,
                        run_id,
                        run_key,
                        accepted_at,
                    } => {
                        [accepted_by_member_id, project_ref, run_id, run_key]
                            .into_iter()
                            .all(|v| id(v))
                            && timestamp(*accepted_at)
                    }
                    Decision::Rejected {
                        decided_by_member_id,
                        decided_at,
                    }
                    | Decision::Cancelled {
                        decided_by_member_id,
                        decided_at,
                    } => id(decided_by_member_id) && timestamp(*decided_at),
                };
                [
                    &value.assignment_id,
                    &value.task_id,
                    &value.offered_by_member_id,
                    &value.target_node_id,
                ]
                .into_iter()
                .all(|v| id(v))
                    && decision_valid
            }
        }
    }
}

fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes
}

fn timestamp(value: i64) -> bool {
    (0..=253_402_300_799).contains(&value)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum ProtocolError {
    #[error("Invalid collaboration message")]
    InvalidMessage,
    #[error("Collaboration message exceeds the byte limit")]
    MessageTooLarge,
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
