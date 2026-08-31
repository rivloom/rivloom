use super::credential::{CredentialBinding, IssuedCredential, SecretToken};
use super::invitation::{IssuedInvitation, JoinRequest};
use super::node::{Node, NodeError};
use super::protocol::{MAX_REVISION, Message, id, timestamp};
use super::reconcile::MAX_SHARED_RECORDS;
use super::secret_store::{NodeSecrets, SecretBackend, SecretField, VaultError};
use super::server::now;
use super::tls::{Peer, TlsChannel};
use super::wire::{Operation, Reply, Request, Response, WireError};

/// Explicit synchronous Node connection; callers must keep blocking IO off the desktop UI thread.
pub(super) struct Client {
    binding: CredentialBinding,
    node: Node,
    channel: Option<TlsChannel>,
}

impl Client {
    pub(super) fn connect<B: SecretBackend>(
        peer: &Peer,
        vault: &NodeSecrets<B>,
        binding: CredentialBinding,
    ) -> Result<Self, ClientError> {
        let mut client = Self {
            node: Node::new(binding.clone())?,
            binding,
            channel: None,
        };
        client.reconnect(peer, vault)?;
        Ok(client)
    }

    /// A lost Join response or failed vault write requires owner recovery, never automatic re-enrollment.
    pub(super) fn join<B: SecretBackend>(
        peer: &Peer,
        vault: &NodeSecrets<B>,
        join: JoinRequest<'_>,
    ) -> Result<Self, ClientError> {
        let mut channel = TlsChannel::connect(peer).map_err(|_| ClientError::Transport)?;
        let reply = exchange(
            &mut channel,
            Operation::Join {
                brain_id: join.brain_id.into(),
                invitation_id: join.invitation_id.into(),
                secret: SecretField(
                    SecretToken::parse(join.secret.expose_secret())
                        .map_err(|_| ClientError::Invalid)?,
                ),
                identity_id: join.identity_id.into(),
                device_id: join.device_id.into(),
                display_name: join.display_name.into(),
            },
        )?;
        let Reply::Joined {
            binding,
            expires_at,
            secret,
        } = reply
        else {
            return Err(ClientError::Invalid);
        };
        if binding.brain_id != join.brain_id || binding.device_id != join.device_id {
            return Err(ClientError::Invalid);
        }
        let node = Node::new(binding.clone())?;
        let credential = IssuedCredential {
            binding: binding.clone(),
            expires_at,
            secret: secret.0,
        };
        vault.save_new(&credential, now()?)?;
        let mut client = Self {
            binding,
            node,
            channel: Some(channel),
        };
        client.authenticate(credential)?;
        Ok(client)
    }

    pub(super) fn binding(&self) -> &CredentialBinding {
        &self.binding
    }
    pub(super) fn view(&self) -> &Node {
        &self.node
    }

    /// Preserve the last complete revision and confirmed pending message; never generate a new Run.
    pub(super) fn reconnect<B: SecretBackend>(
        &mut self,
        peer: &Peer,
        vault: &NodeSecrets<B>,
    ) -> Result<(), ClientError> {
        self.disconnect();
        let credential = vault.load(&self.binding, now()?)?;
        self.channel = Some(TlsChannel::connect(peer).map_err(|_| ClientError::Transport)?);
        self.authenticate(credential)
    }

    fn authenticate(&mut self, credential: IssuedCredential) -> Result<(), ClientError> {
        match self.request(Operation::Authenticate {
            binding: credential.binding,
            secret: SecretField(credential.secret),
        })? {
            Reply::Authenticated {} => self.reconcile(),
            _ => self.fail(ClientError::Invalid),
        }
    }

    pub(super) fn reconcile(&mut self) -> Result<(), ClientError> {
        for _ in 0..MAX_SHARED_RECORDS {
            let reply = self.request(Operation::Sync(self.node.reconcile_request()))?;
            let Reply::Page(page) = reply else {
                return self.fail(ClientError::Invalid);
            };
            match self.node.accept_page(*page) {
                Ok(None) => return Ok(()),
                Ok(Some(_)) => {}
                Err(error) => return self.fail(error.into()),
            }
        }
        self.fail(ClientError::Invalid)
    }

    pub(super) fn pulse(&mut self) -> Result<(), ClientError> {
        match self.request(Operation::Pulse {})? {
            Reply::Pulsed { revision } if (1..=MAX_REVISION).contains(&revision) => {
                self.reconcile()
            }
            _ => self.fail(ClientError::Invalid),
        }
    }

    pub(super) fn submit_confirmed(&mut self, message: Message) -> Result<u64, ClientError> {
        self.node.queue_confirmed(message)?;
        self.publish_pending()
    }

    /// Explicit retry after reconciliation, preserving the confirmed key and payload hash.
    pub(super) fn publish_pending(&mut self) -> Result<u64, ClientError> {
        let outgoing = self.node.outgoing()?;
        let Reply::Applied { key, revision } =
            self.request(Operation::Submit(Box::new(outgoing)))?
        else {
            return self.fail(ClientError::Invalid);
        };
        if let Err(error) = self.node.acknowledge(&key, revision) {
            return self.fail(error.into());
        }
        self.reconcile()?;
        Ok(revision)
    }

    pub(super) fn invite(&mut self) -> Result<IssuedInvitation, ClientError> {
        let Reply::Invited {
            brain_id,
            invitation_id,
            expires_at,
            secret,
        } = self.request(Operation::Invite {})?
        else {
            return self.fail(ClientError::Invalid);
        };
        let time = match now() {
            Ok(time) => time,
            Err(error) => return self.fail(error.into()),
        };
        if brain_id != self.binding.brain_id
            || SecretToken::parse(&invitation_id).is_err()
            || !timestamp(expires_at)
            || expires_at <= time
            || expires_at - time > 600
        {
            return self.fail(ClientError::Invalid);
        }
        Ok(IssuedInvitation {
            brain_id,
            invitation_id,
            expires_at,
            secret: secret.0,
        })
    }

    pub(super) fn cancel_invite(&mut self, invitation_id: String) -> Result<(), ClientError> {
        if !id(&invitation_id) {
            return Err(ClientError::Invalid);
        }
        self.administer(Operation::CancelInvite { invitation_id })
    }

    pub(super) fn revoke(&mut self, member_id: String) -> Result<(), ClientError> {
        if !id(&member_id) || !self.node.is_ready() {
            return Err(ClientError::Invalid);
        }
        self.administer(Operation::Revoke {
            member_id,
            revision: self.node.revision(),
        })
    }

    fn administer(&mut self, operation: Operation) -> Result<(), ClientError> {
        match self.request(operation)? {
            Reply::Administered { revision } if (1..=MAX_REVISION).contains(&revision) => {
                self.reconcile()
            }
            _ => self.fail(ClientError::Invalid),
        }
    }

    fn request(&mut self, operation: Operation) -> Result<Reply, ClientError> {
        let result = self
            .channel
            .as_mut()
            .ok_or(ClientError::Transport)
            .and_then(|channel| exchange(channel, operation));
        match result {
            Ok(reply) => Ok(reply),
            Err(error) => self.fail(error),
        }
    }

    fn fail<T>(&mut self, error: ClientError) -> Result<T, ClientError> {
        self.disconnect();
        Err(error)
    }

    pub(super) fn disconnect(&mut self) {
        if let Some(mut channel) = self.channel.take() {
            channel.close();
        }
        self.node.disconnect();
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn exchange(channel: &mut TlsChannel, operation: Operation) -> Result<Reply, ClientError> {
    let request = Request {
        version: 1,
        id: SecretToken::generate()
            .map_err(|_| ClientError::Invalid)?
            .expose_secret()
            .into(),
        operation,
    };
    let bytes = request.encode()?;
    channel.send(&bytes).map_err(|_| ClientError::Transport)?;
    drop(bytes);
    let bytes = channel.receive().map_err(|_| ClientError::Transport)?;
    match Response::decode(&bytes, &request.id)?.result {
        Reply::Error(error) => Err(error.into()),
        reply => Ok(reply),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum ClientError {
    #[error("Invalid collaboration response")]
    Invalid,
    #[error("Collaboration transport disconnected")]
    Transport,
    #[error("Collaboration request rejected: {0:?}")]
    Peer(WireError),
    #[error(transparent)]
    Node(#[from] NodeError),
    #[error(transparent)]
    Vault(#[from] VaultError),
}

impl From<WireError> for ClientError {
    fn from(error: WireError) -> Self {
        Self::Peer(error)
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
