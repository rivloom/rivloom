use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

use super::host_profile::HostProfile;
use super::hosting::{BrainService, HostingError, HostingStatus};
use super::secret_store::{NativeVault, SecretBackend};
use crate::identity::IdentityService;

pub(crate) struct DesktopBrainState {
    pub(super) service: BrainService<Arc<dyn SecretBackend + Send + Sync>>,
}

impl DesktopBrainState {
    pub(crate) fn new(directory: PathBuf) -> Result<Self, HostingError> {
        let backend: Arc<dyn SecretBackend + Send + Sync> = Arc::new(NativeVault);
        Ok(Self {
            service: BrainService::new(directory, backend)?,
        })
    }
    pub(crate) fn shutdown(&self) {
        self.service.shutdown();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InitializeBrainParams {
    address: std::net::SocketAddr,
    server_name: String,
}

#[tauri::command]
pub(crate) async fn get_local_brain_status<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<HostingStatus, HostingError> {
    run_blocking(app_handle, window, |_, state| state.service.status()).await
}

#[tauri::command]
pub(crate) async fn initialize_local_brain<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
    params: InitializeBrainParams,
) -> Result<HostProfile, HostingError> {
    run_blocking(app_handle, window, move |app, state| {
        let identity = app
            .try_state::<IdentityService>()
            .ok_or(HostingError::Unavailable)?
            .get()
            .map_err(|_| HostingError::Unavailable)?;
        state
            .service
            .initialize(&identity, params.address, &params.server_name)
    })
    .await
}

#[tauri::command]
pub(crate) async fn start_local_brain<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<HostingStatus, HostingError> {
    run_blocking(app_handle, window, |app, state| {
        let identity = app
            .try_state::<IdentityService>()
            .ok_or(HostingError::Unavailable)?
            .get()
            .map_err(|_| HostingError::Unavailable)?;
        state.service.start(&identity)
    })
    .await
}

#[tauri::command]
pub(crate) async fn stop_local_brain<R: Runtime>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<(), HostingError> {
    run_blocking(app_handle, window, |_, state| state.service.stop()).await
}

async fn run_blocking<R, T>(
    app_handle: AppHandle<R>,
    window: WebviewWindow<R>,
    operation: impl FnOnce(&AppHandle<R>, &DesktopBrainState) -> Result<T, HostingError>
    + Send
    + 'static,
) -> Result<T, HostingError>
where
    R: Runtime,
    T: Send + 'static,
{
    if window.label() != "main" {
        return Err(HostingError::Invalid);
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle
            .try_state::<DesktopBrainState>()
            .ok_or(HostingError::Unavailable)?;
        operation(&app_handle, state.inner())
    })
    .await
    .map_err(|_| HostingError::Unavailable)?
}

#[cfg(all(test, any(not(windows), feature = "test-tauri-commands")))]
#[path = "commands_tests.rs"]
mod tests;
