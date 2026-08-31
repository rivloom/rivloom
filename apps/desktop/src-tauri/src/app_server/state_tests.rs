use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::app_server::event_router::RunEventKind;

#[test]
fn app_server_observer_routes_run_notifications_without_replacing_account_observation() {
    let events = Arc::new(EventRouter::default());
    let connection_identity = ConnectionIdentity::new();
    let stream = events
        .prepare_run(connection_identity.clone(), "run-1", "thread-1")
        .unwrap();
    let observer = AppServerObserver {
        account_service: Arc::new(AccountService::new()),
        events,
    };

    observer.on_notification(
        &connection_identity,
        "turn/started",
        &json!({
            "threadId": "thread-1",
            "turn": {"id": "turn-1", "status": "inProgress"}
        }),
    );

    assert_eq!(stream.try_recv().unwrap().kind, RunEventKind::Running);
}
