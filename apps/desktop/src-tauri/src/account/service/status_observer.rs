use std::sync::Arc;
use std::sync::PoisonError;

use super::AccountService;
use super::AccountServiceState;
use crate::account::types::AccountStatus;
use crate::app_server::ConnectionControl;
use crate::app_server::process::ConnectionObserver;

/// Receives sanitized account states whenever backend truth or local lifecycle state changes.
///
/// Callbacks are serialized. Implementations must return promptly, must not re-enter account status
/// publication, and must not retain or derive unexposed protocol payloads.
pub(crate) trait AccountStatusObserver: Send + Sync {
    fn on_status(&self, status: &AccountStatus);
}

#[cfg(test)]
pub(super) struct NoopAccountStatusObserver;

#[cfg(test)]
impl AccountStatusObserver for NoopAccountStatusObserver {
    fn on_status(&self, _status: &AccountStatus) {}
}

impl AccountService {
    pub(crate) fn publish_status(&self, status: &AccountStatus) {
        self.publish_status_if(status, |state| &state.status == status);
    }

    fn publish_status_if(
        &self,
        status: &AccountStatus,
        is_current: impl FnOnce(&AccountServiceState) -> bool,
    ) {
        let mut published_status = self
            .inner
            .published_status
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let is_current = {
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            is_current(&state)
        };
        if !is_current || published_status.as_ref() == Some(status) {
            return;
        }
        *published_status = Some(status.clone());
        self.inner.status_observer.on_status(status);
    }
}

impl ConnectionObserver for AccountService {
    fn on_connected(&self, connection: Arc<dyn ConnectionControl>) {
        self.connect(connection);
    }

    fn on_disconnected(&self) {
        self.disconnect();
    }
}
