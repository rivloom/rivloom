use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::brain::{Brain, BrainError};
use super::credential::ConnectionIdentity;
use super::protocol::{Message, PayloadView, TaskStatus, id};

const MAX_TASKS: usize = 64;
const MAX_REPLAYS: usize = 256;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TaskRecord {
    source: Message,
    pub(super) status: TaskStatus,
    pub(super) revision: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReplayRecord {
    sender_node_id: String,
    key: String,
    fingerprint: [u8; 32],
    revision: u64,
}

impl Brain {
    /// Network admission permits only authenticated Node announcements/heartbeats and new draft Tasks.
    /// Assignment, Run result and owner claims require later, independently authorized workflows.
    pub(super) fn apply(
        &mut self,
        session: &ConnectionIdentity,
        message: Message,
        now: i64,
    ) -> Result<u64, BrainError> {
        self.observe(now)?;
        self.state.credentials.authorize_task(session, now)?;
        let admission = message.admission();
        let binding = session.binding();
        if admission.brain_id != self.brain_id() || admission.sender_node_id != binding.node_id {
            return Err(BrainError::Invalid);
        }
        let fingerprint = message.payload_hash().map_err(|_| BrainError::Invalid)?;
        if let Some(previous) =
            self.state.replays.iter().find(|record| {
                record.sender_node_id == binding.node_id && record.key == admission.key
            })
        {
            return if previous.fingerprint == fingerprint {
                Ok(previous.revision)
            } else {
                Err(BrainError::Conflict)
            };
        }
        self.check_revision(admission.revision)?;
        if self.state.replays.len() >= MAX_REPLAYS {
            return Err(BrainError::Capacity);
        }
        let revision = self.next_revision()?;
        match admission.payload {
            PayloadView::Node {
                node_id,
                member_id,
                device_id,
            } => {
                if node_id != binding.node_id
                    || member_id != binding.member_id
                    || device_id != binding.device_id
                {
                    return Err(BrainError::Invalid);
                }
                self.heartbeat(session, admission.revision, now)?;
                self.state
                    .nodes
                    .get_mut(node_id)
                    .ok_or(BrainError::Invalid)?
                    .announcement = Some(message.clone());
            }
            PayloadView::Task {
                task_id,
                member_id,
                status,
            } => {
                if member_id != binding.member_id || *status != TaskStatus::Draft {
                    return Err(BrainError::Invalid);
                }
                if self.state.tasks.contains_key(task_id) {
                    return Err(BrainError::Conflict);
                }
                if self.state.tasks.len() >= MAX_TASKS {
                    return Err(BrainError::Capacity);
                }
                self.state.tasks.insert(
                    task_id.to_owned(),
                    TaskRecord {
                        source: message.clone(),
                        status: TaskStatus::Draft,
                        revision,
                    },
                );
                self.state.revision = revision;
            }
            PayloadView::Unsupported => return Err(BrainError::Invalid),
        }
        self.state.replays.push(ReplayRecord {
            sender_node_id: binding.node_id.clone(),
            key: admission.key.into(),
            fingerprint,
            revision,
        });
        Ok(revision)
    }

    /// Trusted coordinator only: R4 must verify offer/accept/Run/receipt relationships before calling.
    /// This changes a record and never starts or resumes a Runtime.
    pub(super) fn set_task_status(
        &mut self,
        task_id: &str,
        expected_revision: u64,
        next: TaskStatus,
        now: i64,
    ) -> Result<u64, BrainError> {
        self.observe(now)?;
        self.check_revision(expected_revision)?;
        let task = self.state.tasks.get(task_id).ok_or(BrainError::Invalid)?;
        if task.status == next {
            return Ok(self.revision());
        }
        let allowed = match task.status {
            TaskStatus::Draft => matches!(next, TaskStatus::Offered | TaskStatus::Cancelled),
            TaskStatus::Offered => matches!(
                next,
                TaskStatus::Accepted | TaskStatus::Rejected | TaskStatus::Cancelled
            ),
            TaskStatus::Accepted => matches!(
                next,
                TaskStatus::Running | TaskStatus::Cancelled | TaskStatus::OutcomeUnknown
            ),
            TaskStatus::Running => matches!(
                next,
                TaskStatus::AwaitingReview
                    | TaskStatus::Failed
                    | TaskStatus::Cancelled
                    | TaskStatus::OutcomeUnknown
            ),
            TaskStatus::AwaitingReview => {
                matches!(next, TaskStatus::Approved | TaskStatus::Rejected)
            }
            TaskStatus::OutcomeUnknown => matches!(
                next,
                TaskStatus::AwaitingReview | TaskStatus::Failed | TaskStatus::Cancelled
            ),
            TaskStatus::Approved
            | TaskStatus::Rejected
            | TaskStatus::Cancelled
            | TaskStatus::Failed => false,
        };
        if !allowed {
            return Err(BrainError::Invalid);
        }
        let revision = self.next_revision()?;
        let task = self
            .state
            .tasks
            .get_mut(task_id)
            .ok_or(BrainError::Invalid)?;
        task.status = next;
        task.revision = revision;
        self.state.revision = revision;
        Ok(revision)
    }

    pub(super) fn validate_tasks_and_replays(&self) -> Result<(), BrainError> {
        if self.state.tasks.len() > MAX_TASKS || self.state.replays.len() > MAX_REPLAYS {
            return Err(BrainError::Capacity);
        }
        let mut keys = BTreeSet::new();
        let mut revisions = BTreeSet::new();
        for record in &self.state.replays {
            if !id(&record.key)
                || !self.state.nodes.contains_key(&record.sender_node_id)
                || record.revision == 0
                || record.revision > self.revision()
                || !keys.insert((&record.sender_node_id, &record.key))
                || !revisions.insert(record.revision)
            {
                return Err(BrainError::Invalid);
            }
        }
        for (key, task) in &self.state.tasks {
            let source = task.source.admission();
            let PayloadView::Task {
                task_id,
                member_id,
                status,
            } = source.payload
            else {
                return Err(BrainError::Invalid);
            };
            if key != task_id
                || *status != TaskStatus::Draft
                || source.brain_id != self.brain_id()
                || task.revision > self.revision()
                || source.revision >= task.revision
                || !self
                    .state
                    .nodes
                    .get(source.sender_node_id)
                    .is_some_and(|node| node.member_id == member_id)
                || !self.state.replays.iter().any(|record| {
                    record.sender_node_id == source.sender_node_id
                        && record.key == source.key
                        && record.revision == source.revision + 1
                        && task
                            .source
                            .payload_hash()
                            .is_ok_and(|hash| hash == record.fingerprint)
                })
            {
                return Err(BrainError::Invalid);
            }
        }
        for (key, node) in &self.state.nodes {
            if let Some(message) = &node.announcement {
                let source = message.admission();
                let PayloadView::Node {
                    node_id,
                    member_id,
                    device_id,
                } = source.payload
                else {
                    return Err(BrainError::Invalid);
                };
                if key != node_id
                    || source.sender_node_id != node_id
                    || source.brain_id != self.brain_id()
                    || member_id != node.member_id
                    || device_id != node.device_id
                    || source.revision >= node.revision
                {
                    return Err(BrainError::Invalid);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "brain_task_tests.rs"]
mod tests;
