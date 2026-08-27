use std::sync::Arc;
use std::sync::Mutex;

use pretty_assertions::assert_eq;

use super::AccountService;
use super::AccountStatusObserver;
use super::account_actions_tests::signed_out_response;
use super::retryable_account_error;
use super::tests::FakeConnection;
use super::tests::FakeUrlOpener;
use crate::account::types::AccountStatus;

#[test]
fn connection_refresh_and_disconnect_publish_deduplicated_normalized_statuses() {
    let observer = Arc::new(RecordingAccountStatusObserver::default());
    let service = AccountService::with_runtime_dependencies(
        Arc::new(FakeUrlOpener::new(vec![])),
        observer.clone(),
    );
    service.connect(Arc::new(FakeConnection::new(vec![
        signed_out_response(),
        signed_out_response(),
    ])));
    service.refresh();
    service.refresh();
    service.disconnect();

    assert_eq!(
        observer.statuses(),
        vec![
            AccountStatus::Checking,
            AccountStatus::SignedOut,
            retryable_account_error(),
        ]
    );
}

#[derive(Default)]
struct RecordingAccountStatusObserver {
    statuses: Mutex<Vec<AccountStatus>>,
}

impl RecordingAccountStatusObserver {
    fn statuses(&self) -> Vec<AccountStatus> {
        self.statuses.lock().unwrap().clone()
    }
}

impl AccountStatusObserver for RecordingAccountStatusObserver {
    fn on_status(&self, status: &AccountStatus) {
        self.statuses.lock().unwrap().push(status.clone());
    }
}
