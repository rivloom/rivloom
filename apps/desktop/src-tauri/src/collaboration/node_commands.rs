use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

use super::commands::DesktopBrainState;
use super::hosting::HostingStatus;
use super::node_membership::{InvitationDisplay, InvitationSecret, MemberDirectory};
use super::node_registration::NodeRegistration;
use super::node_session::{NodeSession, NodeStatus, SessionError};
use super::secret_store::{NativeVault, SecretBackend};
use crate::identity::{IdentityService, RivloomIdentity};

pub(crate) struct DesktopNodeState {
    session: NodeSession<Arc<dyn SecretBackend + Send + Sync>>,
}
impl DesktopNodeState {
    pub(crate) fn new(directory: PathBuf) -> Result<Self, SessionError> {
        let vault: Arc<dyn SecretBackend + Send + Sync> = Arc::new(NativeVault);
        Ok(Self {
            session: NodeSession::new(directory, vault)?,
        })
    }
    pub(crate) fn shutdown(&self) {
        self.session.shutdown();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct JoinBrainParams {
    descriptor: String,
    confirmed_fingerprint: String,
    invitation: IncomingInvitation,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IncomingInvitation {
    brain_id: String,
    invitation_id: String,
    expires_at: i64,
    secret: InvitationSecret,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OwnerParams {
    confirmed_fingerprint: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CancelInvitationParams {
    invitation_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RevokeMemberParams {
    member_id: String,
}

#[tauri::command]
pub(crate) async fn get_node_status<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<NodeStatus, SessionError> {
    run(app_handle, window, |_, state, identity| {
        state.session.status(identity)
    })
    .await
}

#[tauri::command]
pub(crate) async fn join_brain<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
    params: JoinBrainParams,
) -> Result<NodeStatus, SessionError> {
    run(app_handle, window, move |_, state, identity| {
        let registration = NodeRegistration::confirmed(
            identity,
            params.descriptor.as_bytes(),
            &params.confirmed_fingerprint,
        )?;
        let invitation = params.invitation;
        let now = super::server::now().map_err(|_| SessionError::Unavailable)?;
        if invitation.brain_id != registration.descriptor.brain_id()
            || !super::protocol::timestamp(invitation.expires_at)
            || invitation.expires_at <= now
            || invitation.expires_at - now > 600
        {
            return Err(SessionError::Invalid);
        }
        state.session.join(
            identity,
            &registration,
            &invitation.invitation_id,
            &invitation.secret.0,
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn connect_brain_owner<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
    params: OwnerParams,
) -> Result<NodeStatus, SessionError> {
    run(app_handle, window, move |app, state, identity| {
        let brain = app
            .try_state::<DesktopBrainState>()
            .ok_or(SessionError::Unavailable)?;
        let HostingStatus::Running(profile) = brain
            .service
            .status()
            .map_err(|_| SessionError::Unavailable)?
        else {
            return Err(SessionError::Unavailable);
        };
        state
            .session
            .connect_owner(identity, &profile, &params.confirmed_fingerprint)
    })
    .await
}

#[tauri::command]
pub(crate) async fn connect_brain<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<NodeStatus, SessionError> {
    run(app_handle, window, |_, state, identity| {
        state.session.connect(identity)
    })
    .await
}

#[tauri::command]
pub(crate) async fn refresh_brain<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<NodeStatus, SessionError> {
    run(app_handle, window, |_, state, identity| {
        state.session.refresh(identity)
    })
    .await
}

#[tauri::command]
pub(crate) async fn disconnect_brain<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<(), SessionError> {
    run(app_handle, window, |_, state, _| state.session.disconnect()).await
}

#[tauri::command]
pub(crate) async fn list_brain_members<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<MemberDirectory, SessionError> {
    run(app_handle, window, |_, state, identity| {
        state.session.members(identity)
    })
    .await
}

#[tauri::command]
pub(crate) async fn create_brain_invitation<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<InvitationDisplay, SessionError> {
    run(app_handle, window, |_, state, identity| {
        state.session.invite(identity)
    })
    .await
}

#[tauri::command]
pub(crate) async fn cancel_brain_invitation<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
    params: CancelInvitationParams,
) -> Result<(), SessionError> {
    run(app_handle, window, move |_, state, identity| {
        state.session.cancel_invite(identity, params.invitation_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn revoke_brain_member<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
    params: RevokeMemberParams,
) -> Result<(), SessionError> {
    run(app_handle, window, move |_, state, identity| {
        state.session.revoke(identity, params.member_id)
    })
    .await
}

async fn run<R: Runtime, T: Send + 'static>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
    operation: impl FnOnce(
        &AppHandle<R>,
        &DesktopNodeState,
        &RivloomIdentity,
    ) -> Result<T, SessionError>
    + Send
    + 'static,
) -> Result<T, SessionError> {
    if window.label() != "main" {
        return Err(SessionError::Invalid);
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle
            .try_state::<DesktopNodeState>()
            .ok_or(SessionError::Unavailable)?;
        let identity = app_handle
            .try_state::<IdentityService>()
            .ok_or(SessionError::Unavailable)?
            .get()
            .map_err(|_| SessionError::Unavailable)?;
        operation(&app_handle, state.inner(), &identity)
    })
    .await
    .map_err(|_| SessionError::Unavailable)?
}

#[cfg(all(test, any(not(windows), feature = "test-tauri-commands")))]
#[path = "node_commands_tests.rs"]
mod tests;
