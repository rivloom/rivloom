use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_MESSAGE_BYTES: usize = 32 * 1024;
const MAX_PATCH_BYTES: u64 = 512 * 1024;
pub(super) const MAX_REVISION: u64 = 9_007_199_254_740_991;
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

// Serde's derived unit enums also accept {"variant": null}; v1 only accepts strings.
macro_rules! string_enum {
    ($name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
        enum $name { $(#[serde(rename = $wire)] $variant),+ }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                match String::deserialize(deserializer)?.as_str() {
                    $($wire => Ok(Self::$variant),)+
                    _ => Err(serde::de::Error::custom("Invalid collaboration enum")),
                }
            }
        }
    };
}

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
    RunReceipt(Receipt),
    Artifact(Artifact),
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

string_enum! { Role {
    Owner => "owner",
    Member => "member",
} }

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

string_enum! { RuntimeId {
    Codex => "codex",
} }

string_enum! { Capability {
    TaskRun => "taskRun",
    Interrupt => "interrupt",
    Patch => "patch",
} }

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

string_enum! { ArtifactKind {
    Patch => "patch",
} }

string_enum! { TaskStatus {
    Draft => "draft",
    Offered => "offered",
    Accepted => "accepted",
    Running => "running",
    AwaitingReview => "awaitingReview",
    Approved => "approved",
    Rejected => "rejected",
    Cancelled => "cancelled",
    Failed => "failed",
    OutcomeUnknown => "outcomeUnknown",
} }

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

string_enum! { ExecutionPolicy {
    ManagedWorktreeOffline => "managedWorktreeOffline",
} }

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Artifact {
    artifact_id: String,
    task_id: String,
    run_id: String,
    baseline_commit: String,
    state: ArtifactState,
    limit_bytes: u64,
    byte_count: Option<u64>,
    sha256: Option<String>,
}

string_enum! { ArtifactState {
    Empty => "empty",
    Complete => "complete",
    TooLarge => "tooLarge",
    UnsupportedEncoding => "unsupportedEncoding",
} }

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    content: ReceiptContent,
    content_sha256: String,
}

// Field order is part of the v1 receipt hash contract. See collaboration-protocol-v1.md.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReceiptContent {
    task_id: String,
    run_id: String,
    node_id: String,
    runtime_id: RuntimeId,
    runtime_version: String,
    started_at: i64,
    finished_at: i64,
    outcome: ReceiptOutcome,
    summary: Option<String>,
    failure: Option<Failure>,
    tests: TestReport,
    artifact: Artifact,
}

string_enum! { ReceiptOutcome {
    Success => "success",
    Failed => "failed",
    Cancelled => "cancelled",
    OutcomeUnknown => "outcomeUnknown",
} }

string_enum! { Failure {
    ExecutionFailed => "executionFailed",
    ConnectionLost => "connectionLost",
    PolicyDenied => "policyDenied",
    InvalidArtifact => "invalidArtifact",
} }

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", deny_unknown_fields)]
enum TestReport {
    NotReported {},
    Reported { executions: Vec<TestExecution> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TestExecution {
    name: String,
    exit_code: i32,
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
            Self::Artifact(value) => value.valid(),
            Self::RunReceipt(value) => value.valid(),
        }
    }
}

impl Artifact {
    fn valid(&self) -> bool {
        let state_valid = match self.state {
            ArtifactState::Empty => {
                self.byte_count == Some(0) && self.sha256.as_deref() == Some(EMPTY_SHA256)
            }
            ArtifactState::Complete | ArtifactState::UnsupportedEncoding => {
                self.byte_count
                    .is_some_and(|count| (1..=MAX_PATCH_BYTES).contains(&count))
                    && self.sha256.as_deref().is_some_and(|value| hex(value, 64))
            }
            ArtifactState::TooLarge => self.byte_count.is_none() && self.sha256.is_none(),
        };
        [&self.artifact_id, &self.task_id, &self.run_id]
            .into_iter()
            .all(|v| id(v))
            && (hex(&self.baseline_commit, 40) || hex(&self.baseline_commit, 64))
            && self.limit_bytes == MAX_PATCH_BYTES
            && state_valid
    }
}

impl Receipt {
    fn valid(&self) -> bool {
        let content = &self.content;
        let tests_valid = match &content.tests {
            TestReport::NotReported {} => true,
            TestReport::Reported { executions } => {
                executions.len() <= 32
                    && executions.iter().all(|test| text(&test.name, 256))
                    && executions.iter().map(|test| test.name.len()).sum::<usize>() <= 4096
            }
        };
        let outcome_valid = match content.outcome {
            ReceiptOutcome::Success | ReceiptOutcome::Cancelled => content.failure.is_none(),
            ReceiptOutcome::Failed | ReceiptOutcome::OutcomeUnknown => content.failure.is_some(),
        };
        [&content.task_id, &content.run_id, &content.node_id]
            .into_iter()
            .all(|v| id(v))
            && text(&content.runtime_version, 128)
            && timestamp(content.started_at)
            && timestamp(content.finished_at)
            && content.finished_at >= content.started_at
            && content
                .summary
                .as_deref()
                .is_none_or(|value| text(value, 4096))
            && content.artifact.valid()
            && content.task_id == content.artifact.task_id
            && content.run_id == content.artifact.run_id
            && tests_valid
            && outcome_valid
            && serde_json::to_vec(content)
                .is_ok_and(|bytes| format!("{:x}", Sha256::digest(bytes)) == self.content_sha256)
    }
}

pub(super) fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn timestamp(value: i64) -> bool {
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
