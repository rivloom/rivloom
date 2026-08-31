use std::collections::{BTreeMap, BTreeSet};

use super::credential::CredentialBinding;
use super::protocol::{MAX_REVISION, Message, PayloadView, TaskStatus, id};
use super::reconcile::{MAX_SHARED_RECORDS, Page, ReconcileRequest, SharedData, SharedRecord};

/// Local collaboration view and one explicitly confirmed retry. Never owns or starts a Runtime Run.
pub(super) struct Node {
    binding: CredentialBinding,
    revision: u64,
    records: BTreeMap<String, SharedRecord>,
    sync: Option<PendingSync>,
    ready: bool,
    uncertain_tasks: BTreeSet<String>,
    pending_message: Option<Message>,
}

struct PendingSync {
    request: ReconcileRequest,
    records: BTreeMap<String, SharedRecord>,
    seen: BTreeSet<String>,
}

impl Node {
    pub(super) fn new(binding: CredentialBinding) -> Result<Self, NodeError> {
        if [
            &binding.brain_id,
            &binding.member_id,
            &binding.node_id,
            &binding.device_id,
        ]
        .into_iter()
        .any(|v| !id(v))
        {
            return Err(NodeError::Invalid);
        }
        Ok(Self {
            binding,
            revision: 0,
            records: BTreeMap::new(),
            sync: None,
            ready: false,
            uncertain_tasks: BTreeSet::new(),
            pending_message: None,
        })
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) fn is_ready(&self) -> bool {
        self.ready
    }

    pub(super) fn reconcile_request(&self) -> ReconcileRequest {
        self.sync
            .as_ref()
            .map(|sync| sync.request.clone())
            .unwrap_or(ReconcileRequest {
                after: self.revision,
                at: None,
                offset: 0,
            })
    }

    pub(super) fn accept_page(
        &mut self,
        page: Page,
    ) -> Result<Option<ReconcileRequest>, NodeError> {
        self.ready = false;
        let mut pending = self.sync.take().unwrap_or_else(|| PendingSync {
            request: ReconcileRequest {
                after: self.revision,
                at: None,
                offset: 0,
            },
            records: self.records.clone(),
            seen: BTreeSet::new(),
        });
        page.validate().map_err(|_| NodeError::Invalid)?;
        if page.brain_id != self.binding.brain_id
            || page.member_id != self.binding.member_id
            || page.after != pending.request.after
            || page.offset != pending.request.offset
            || pending.request.at.is_some_and(|at| at != page.at)
        {
            return Err(NodeError::Invalid);
        }
        for record in page.records {
            let key = match &record.data {
                SharedData::Member { member_id, .. } => format!("member:{member_id}"),
                SharedData::Node { node_id, .. } => format!("node:{node_id}"),
                SharedData::Task { task_id, .. } => format!("task:{task_id}"),
            };
            if !pending.seen.insert(key.clone()) {
                return Err(NodeError::Invalid);
            }
            pending.records.insert(key, record);
        }
        if pending.records.len() > usize::from(MAX_SHARED_RECORDS)
            || serde_json::to_vec(&pending.records)
                .map_err(|_| NodeError::Invalid)?
                .len()
                > 3 * 1024 * 1024
        {
            return Err(NodeError::Invalid);
        }
        if let Some(offset) = page.next {
            pending.request = ReconcileRequest {
                after: page.after,
                at: Some(page.at),
                offset,
            };
            let next = pending.request.clone();
            self.sync = Some(pending);
            return Ok(Some(next));
        }
        let mut counts = [0usize; 3];
        let mut owners = 0;
        for record in pending.records.values() {
            match &record.data {
                SharedData::Member { owner, revoked, .. } => {
                    counts[0] += 1;
                    if *owner && !revoked {
                        owners += 1;
                    }
                }
                SharedData::Node {
                    member_id, online, ..
                } => {
                    counts[1] += 1;
                    if !matches!(pending.records.get(&format!("member:{member_id}")).map(|r| &r.data),
                        Some(SharedData::Member { revoked, .. }) if !online || !revoked)
                    {
                        return Err(NodeError::Invalid);
                    }
                }
                SharedData::Task { source, .. } => {
                    counts[2] += 1;
                    if !matches!(pending.records.get(&format!("node:{}", source.admission().sender_node_id)).map(|r| &r.data),
                        Some(SharedData::Node { member_id, .. }) if member_id == &self.binding.member_id)
                    {
                        return Err(NodeError::Invalid);
                    }
                }
            }
        }
        if owners != 1
            || counts.into_iter().any(|count| count > 64)
            || !matches!(
                pending
                    .records
                    .get(&format!("member:{}", self.binding.member_id))
                    .map(|r| &r.data),
                Some(SharedData::Member { revoked: false, .. })
            )
            || !matches!(pending.records.get(&format!("node:{}", self.binding.node_id)).map(|r| &r.data),
                Some(SharedData::Node { member_id, device_id, .. })
                    if member_id == &self.binding.member_id && device_id == &self.binding.device_id)
        {
            return Err(NodeError::Invalid);
        }
        self.records = pending.records;
        self.revision = page.at;
        self.ready = true;
        Ok(None)
    }

    pub(super) fn disconnect(&mut self) {
        self.ready = false;
        self.sync = None;
        for record in self.records.values() {
            if let SharedData::Task {
                task_id,
                status: TaskStatus::Running,
                ..
            } = &record.data
            {
                self.uncertain_tasks.insert(task_id.clone());
            }
        }
    }

    /// A reconnect alone cannot prove a running task's result. R2/R4 own durable Run reconciliation.
    pub(super) fn task_status(&self, task_id: &str) -> Option<TaskStatus> {
        match &self.records.get(&format!("task:{task_id}"))?.data {
            SharedData::Task {
                status: TaskStatus::Running,
                ..
            } if !self.ready || self.uncertain_tasks.contains(task_id) => {
                Some(TaskStatus::OutcomeUnknown)
            }
            SharedData::Task { status, .. } => Some(status.clone()),
            SharedData::Member { .. } | SharedData::Node { .. } => None,
        }
    }

    /// Only content explicitly confirmed for sharing belongs here. Receipt admission remains an R4 gate.
    pub(super) fn queue_confirmed(&mut self, message: Message) -> Result<(), NodeError> {
        let a = message.admission();
        let b = &self.binding;
        let allowed = match a.payload {
            PayloadView::Node {
                node_id,
                member_id,
                device_id,
            } => node_id == b.node_id && member_id == b.member_id && device_id == b.device_id,
            PayloadView::Task {
                member_id, status, ..
            } => member_id == b.member_id && *status == TaskStatus::Draft,
            PayloadView::Unsupported => message.is_receipt_from(&b.node_id),
        };
        if a.brain_id != b.brain_id || a.sender_node_id != b.node_id || !allowed {
            return Err(NodeError::Invalid);
        }
        if let Some(pending) = &self.pending_message {
            return if pending.admission().key == a.key
                && pending.payload_hash() == message.payload_hash()
            {
                Ok(())
            } else {
                Err(NodeError::Busy)
            };
        }
        self.pending_message = Some(message);
        Ok(())
    }

    pub(super) fn outgoing(&self) -> Result<Message, NodeError> {
        if !self.ready {
            return Err(NodeError::Unavailable);
        }
        self.pending_message
            .as_ref()
            .ok_or(NodeError::Unavailable)?
            .with_revision(self.revision)
            .map_err(|_| NodeError::Invalid)
    }

    pub(super) fn acknowledge(&mut self, key: &str, revision: u64) -> Result<(), NodeError> {
        if revision == 0
            || revision > MAX_REVISION
            || !self
                .pending_message
                .as_ref()
                .is_some_and(|message| message.admission().key == key)
        {
            return Err(NodeError::Invalid);
        }
        self.pending_message = None;
        self.ready = false;
        self.sync = None;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum NodeError {
    #[error("Invalid collaboration reconciliation")]
    Invalid,
    #[error("A confirmed collaboration operation is still pending")]
    Busy,
    #[error("Collaboration requires authentication and reconciliation")]
    Unavailable,
}

#[cfg(test)]
#[path = "node_tests.rs"]
mod tests;
