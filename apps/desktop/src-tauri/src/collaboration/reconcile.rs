use serde::{Deserialize, Serialize};

use super::brain::{Brain, BrainError, PresenceReset};
use super::credential::ConnectionIdentity;
use super::protocol::{MAX_REVISION, Message, PayloadView, TaskStatus, id, timestamp};

pub(super) const MAX_CONTROL_BYTES: usize = 64 * 1024;
pub(super) const MAX_SHARED_RECORDS: u16 = 192;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReconcileRequest {
    pub(super) after: u64,
    pub(super) at: Option<u64>,
    pub(super) offset: u16,
}

/// A peer projection, never the durable authority snapshot. One bounded record per page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Page {
    pub(super) version: u32,
    pub(super) brain_id: String,
    pub(super) member_id: String,
    pub(super) after: u64,
    pub(super) at: u64,
    pub(super) offset: u16,
    pub(super) next: Option<u16>,
    pub(super) records: Vec<SharedRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SharedRecord {
    pub(super) revision: u64,
    pub(super) data: SharedData,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum SharedData {
    Member {
        member_id: String,
        identity_id: String,
        display_name: String,
        owner: bool,
        revoked: bool,
    },
    Node {
        node_id: String,
        member_id: String,
        device_id: String,
        online: bool,
        last_seen_at: Option<i64>,
        announcement: Option<Message>,
    },
    Task {
        task_id: String,
        status: TaskStatus,
        source: Message,
    },
}

impl Brain {
    pub(super) fn reconcile(
        &mut self,
        session: &ConnectionIdentity,
        request: ReconcileRequest,
        now: i64,
    ) -> Result<Page, BrainError> {
        self.observe(now)?;
        self.state.credentials.authorize_task(session, now)?;
        self.reset_presence(now, PresenceReset::Expired)?;
        if request.after > self.revision()
            || request.offset >= MAX_SHARED_RECORDS
            || (request.at.is_none() && request.offset != 0)
        {
            return Err(BrainError::Invalid);
        }
        if request.at.is_some_and(|at| at != self.revision()) {
            return Err(BrainError::Conflict);
        }
        let member_id = &session.binding().member_id;
        let members = self.state.members.iter().map(|(key, member)| SharedRecord {
            revision: member.revision,
            data: SharedData::Member {
                member_id: key.clone(),
                identity_id: member.identity_id.clone(),
                display_name: member.display_name.clone(),
                owner: key == &self.state.owner_member_id,
                revoked: member.revoked,
            },
        });
        let nodes = self.state.nodes.iter().map(|(key, node)| SharedRecord {
            revision: node.revision,
            data: SharedData::Node {
                node_id: key.clone(),
                member_id: node.member_id.clone(),
                device_id: node.device_id.clone(),
                online: node.online,
                last_seen_at: node.last_seen_at,
                announcement: node.announcement.clone(),
            },
        });
        // R4 may add explicit assignment participants. Owner is not an implicit task participant.
        let tasks = self.state.tasks.iter().filter(|(_, task)| {
            matches!(task.source.admission().payload, PayloadView::Task { member_id: creator, .. } if creator == member_id)
        }).map(|(key, task)| SharedRecord {
            revision: task.revision,
            data: SharedData::Task { task_id: key.clone(), status: task.status.clone(), source: task.source.clone() },
        });
        let changes: Vec<_> = members
            .chain(nodes)
            .chain(tasks)
            .filter(|record| record.revision > request.after)
            .collect();
        let offset = usize::from(request.offset);
        if offset > changes.len() || (offset == changes.len() && offset != 0) {
            return Err(BrainError::Invalid);
        }
        let page = Page {
            version: 1,
            brain_id: self.brain_id().into(),
            member_id: member_id.clone(),
            after: request.after,
            at: self.revision(),
            offset: request.offset,
            next: (offset + 1 < changes.len()).then_some(request.offset + 1),
            records: changes.into_iter().skip(offset).take(1).collect(),
        };
        page.encode()?;
        Ok(page)
    }
}

impl Page {
    pub(super) fn encode(&self) -> Result<Vec<u8>, BrainError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| BrainError::Invalid)?;
        if bytes.len() > MAX_CONTROL_BYTES {
            return Err(BrainError::Capacity);
        }
        Ok(bytes)
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self, BrainError> {
        if bytes.len() > MAX_CONTROL_BYTES {
            return Err(BrainError::Capacity);
        }
        let page: Self = serde_json::from_slice(bytes).map_err(|_| BrainError::Invalid)?;
        page.validate()?;
        Ok(page)
    }

    pub(super) fn validate(&self) -> Result<(), BrainError> {
        if self.version != 1
            || !id(&self.brain_id)
            || !id(&self.member_id)
            || self.at == 0
            || self.at > MAX_REVISION
            || self.after > self.at
            || self.offset >= MAX_SHARED_RECORDS
            || self.records.len() > 1
            || self
                .next
                .is_some_and(|next| next != self.offset + 1 || next >= MAX_SHARED_RECORDS)
            || (self.records.is_empty() && (self.offset != 0 || self.next.is_some()))
        {
            return Err(BrainError::Invalid);
        }
        for record in &self.records {
            if record.revision <= self.after || record.revision > self.at {
                return Err(BrainError::Invalid);
            }
            let valid = match &record.data {
                SharedData::Member { member_id, identity_id, display_name, .. } =>
                    id(member_id) && id(identity_id) && display_name.len() <= 80
                        && !display_name.trim().is_empty() && !display_name.chars().any(char::is_control),
                SharedData::Node { node_id, member_id, device_id, online, last_seen_at, announcement } => {
                    id(node_id) && id(member_id) && id(device_id)
                        && (!online || last_seen_at.is_some())
                        && last_seen_at.is_none_or(timestamp)
                        && announcement.as_ref().is_none_or(|message| {
                            let admission = message.admission();
                            admission.brain_id == self.brain_id && admission.sender_node_id == node_id
                                && admission.revision < record.revision
                                && matches!(admission.payload, PayloadView::Node { node_id: n, member_id: m, device_id: d }
                                    if n == node_id && m == member_id && d == device_id)
                        })
                }
                SharedData::Task { task_id, source, .. } => {
                    let admission = source.admission();
                    id(task_id) && admission.brain_id == self.brain_id && admission.revision < record.revision
                        && matches!(admission.payload, PayloadView::Task { task_id: t, member_id: m, status: TaskStatus::Draft }
                            if t == task_id && m == self.member_id)
                }
            };
            if !valid {
                return Err(BrainError::Invalid);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "reconcile_tests.rs"]
mod tests;
