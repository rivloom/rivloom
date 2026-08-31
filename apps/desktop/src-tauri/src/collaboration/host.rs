use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::brain::{Brain, BrainError};
use super::credential::{AccessError, ConnectionIdentity};
use super::invitation::JoinRequest;
use super::protocol::id;
use super::secret_store::SecretField;
use super::storage::{BrainStore, StorageError};
use super::wire::{Operation, Reply, Request, Response, WireError};

pub(super) struct Host {
    store: Mutex<BrainStore>,
    active: AtomicUsize,
    admission: Mutex<Limiter>,
    operations: Mutex<Limiter>,
}

impl Host {
    pub(super) fn new(store: BrainStore) -> Arc<Self> {
        Arc::new(Self {
            store: Mutex::new(store),
            active: AtomicUsize::new(0),
            admission: Mutex::new(Limiter::new(/*limit*/ 64, Instant::now())),
            operations: Mutex::new(Limiter::new(/*limit*/ 1024, Instant::now())),
        })
    }

    /// Acquire before TLS handshaking; failed handshakes also consume the global admission budget.
    pub(super) fn session(self: &Arc<Self>) -> Result<HostSession, WireError> {
        let mut admission = self.admission.lock().map_err(|_| WireError::Unavailable)?;
        if self.active.load(Ordering::SeqCst) >= 16 || !admission.take(Instant::now()) {
            return Err(WireError::Busy);
        }
        self.active.fetch_add(1, Ordering::SeqCst);
        Ok(HostSession {
            host: self.clone(),
            phase: Phase::Fresh,
            seen: BTreeSet::new(),
        })
    }
}

enum Phase {
    Fresh,
    Joined,
    Active(ConnectionIdentity),
    Closed,
}

/// One authenticated transport connection. No state or authorization is accepted from peer labels.
pub(super) struct HostSession {
    host: Arc<Host>,
    phase: Phase,
    seen: BTreeSet<String>,
}

impl HostSession {
    pub(super) fn closed(&self) -> bool {
        matches!(self.phase, Phase::Closed)
    }

    /// Caller supplies local trusted time, never a peer timestamp. Release the store lock before IO.
    pub(super) fn handle(&mut self, request: Request, now: i64) -> Response {
        let valid_id = id(&request.id);
        let result = (|| {
            if self.closed() || request.version != 1 || !valid_id {
                return Err(WireError::Rejected);
            }
            if self.seen.len() >= 256 {
                return Err(WireError::Busy);
            }
            if !self.seen.insert(request.id.clone()) {
                return Err(WireError::Rejected);
            }
            if !self
                .host
                .operations
                .lock()
                .map_err(|_| WireError::Unavailable)?
                .take(Instant::now())
            {
                return Err(WireError::Busy);
            }
            let mut store = self.host.store.lock().map_err(|_| WireError::Unavailable)?;
            match request.operation {
                Operation::Authenticate { binding, secret } => {
                    if !matches!(self.phase, Phase::Fresh | Phase::Joined) {
                        return Err(WireError::Rejected);
                    }
                    let identity =
                        store.transact(now, |brain| brain.connect(&binding, &secret.0, now))?;
                    self.phase = Phase::Active(identity);
                    Ok(Reply::Authenticated {})
                }
                Operation::Join {
                    brain_id,
                    invitation_id,
                    secret,
                    identity_id,
                    device_id,
                    display_name,
                } => {
                    if !matches!(self.phase, Phase::Fresh) {
                        return Err(WireError::Rejected);
                    }
                    let enrollment = store.transact(now, |brain| {
                        brain.join(
                            JoinRequest {
                                brain_id: &brain_id,
                                invitation_id: &invitation_id,
                                secret: &secret.0,
                                identity_id: &identity_id,
                                device_id: &device_id,
                                display_name: &display_name,
                            },
                            now,
                        )
                    })?;
                    self.phase = Phase::Joined;
                    Ok(Reply::Joined {
                        binding: enrollment.credential.binding,
                        expires_at: enrollment.credential.expires_at,
                        secret: SecretField(enrollment.credential.secret),
                    })
                }
                operation => {
                    let Phase::Active(identity) = &self.phase else {
                        return Err(WireError::Rejected);
                    };
                    store
                        .transact(now, |brain| {
                            brain.state.credentials.authorize_task(identity, now)?;
                            match operation {
                                Operation::Sync(request) => Ok(Reply::Page(Box::new(
                                    brain.reconcile(identity, request, now)?,
                                ))),
                                Operation::Submit(message) => {
                                    let key = message.admission().key.to_owned();
                                    let revision = brain.apply(identity, *message, now)?;
                                    Ok(Reply::Applied { key, revision })
                                }
                                // A live authenticated TLS pulse is not a retryable business operation.
                                Operation::Pulse {} => Ok(Reply::Pulsed {
                                    revision: brain.heartbeat(identity, brain.revision(), now)?,
                                }),
                                Operation::Invite {} => {
                                    owner(brain, identity, now)?;
                                    let invitation = brain.create_invitation(now)?;
                                    Ok(Reply::Invited {
                                        brain_id: invitation.brain_id,
                                        invitation_id: invitation.invitation_id,
                                        expires_at: invitation.expires_at,
                                        secret: SecretField(invitation.secret),
                                    })
                                }
                                Operation::CancelInvite { invitation_id } => {
                                    owner(brain, identity, now)?;
                                    brain.cancel_invitation(&invitation_id, now)?;
                                    Ok(Reply::Administered {
                                        revision: brain.revision(),
                                    })
                                }
                                Operation::Revoke {
                                    member_id,
                                    revision,
                                } => {
                                    owner(brain, identity, now)?;
                                    Ok(Reply::Administered {
                                        revision: brain.revoke_member(&member_id, revision, now)?,
                                    })
                                }
                                Operation::Authenticate { .. } | Operation::Join { .. } => {
                                    Err(BrainError::Invalid)
                                }
                            }
                        })
                        .map_err(Into::into)
                }
            }
        })();
        let reply = match result {
            Ok(reply) => reply,
            Err(error) => {
                if error != WireError::Conflict {
                    self.phase = Phase::Closed;
                }
                Reply::Error(error)
            }
        };
        Response {
            version: 1,
            id: if valid_id {
                request.id
            } else {
                "invalid".into()
            },
            result: reply,
        }
    }
}

impl Drop for HostSession {
    fn drop(&mut self) {
        self.host.active.fetch_sub(1, Ordering::SeqCst);
    }
}

fn owner(brain: &mut Brain, identity: &ConnectionIdentity, now: i64) -> Result<(), BrainError> {
    brain.state.credentials.authorize_task(identity, now)?;
    if identity.binding().member_id != brain.state.owner_member_id {
        return Err(AccessError::Rejected.into());
    }
    Ok(())
}

impl From<StorageError> for WireError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::State(BrainError::Conflict) => Self::Conflict,
            StorageError::State(
                BrainError::Capacity | BrainError::Access(AccessError::Capacity),
            ) => Self::Busy,
            StorageError::State(
                BrainError::Invalid | BrainError::Access(AccessError::Rejected),
            ) => Self::Rejected,
            StorageError::State(BrainError::Access(AccessError::EntropyUnavailable))
            | StorageError::Read
            | StorageError::Write
            | StorageError::Invalid
            | StorageError::Existing
            | StorageError::Locked
            | StorageError::Changed
            | StorageError::Unavailable => Self::Unavailable,
        }
    }
}

struct Limiter {
    start: Instant,
    remaining: u32,
    limit: u32,
}
impl Limiter {
    fn new(limit: u32, now: Instant) -> Self {
        Self {
            start: now,
            remaining: limit,
            limit,
        }
    }
    fn take(&mut self, now: Instant) -> bool {
        if now.saturating_duration_since(self.start) >= Duration::from_secs(60) {
            self.start = now;
            self.remaining = self.limit;
        }
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
