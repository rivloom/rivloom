use std::sync::Arc;
use std::sync::Mutex;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::*;
use crate::app_server::ConnectionControl;
use crate::app_server::connection::AppServerConnection;

#[test]
fn only_matching_bounded_run_state_is_normalized() {
    let router = EventRouter::default();
    let connection = ConnectionIdentity::new();
    let other_connection = ConnectionIdentity::new();
    let stream = router
        .subscribe_run(connection.clone(), "run-1", "thread-1", "turn-1")
        .unwrap();

    notify(&router, &other_connection, "turn/started", started());
    notify(
        &router,
        &connection,
        "item/agentMessage/delta",
        json!({"threadId": "thread-1", "turnId": "turn-1", "delta": "SECRET"}),
    );
    notify(&router, &connection, "turn/started", started());
    NotificationObserver::on_server_request(
        &router,
        &connection,
        &json!(9),
        "item/commandExecution/requestApproval",
        &json!({"threadId": "thread-1", "turnId": "turn-1", "command": "SECRET"}),
    );
    notify(
        &router,
        &connection,
        "turn/completed",
        json!({
            "threadId": "thread-1",
            "turn": {"id": "turn-1", "status": "completed", "items": ["SECRET"]}
        }),
    );

    assert_eq!(
        drain(&stream),
        vec![
            event(1, RunEventKind::Running),
            event(2, RunEventKind::WaitingApproval),
            event(3, RunEventKind::Completed),
        ]
    );
    assert!(!stream.is_active());
}

#[test]
fn pending_route_binds_the_first_started_turn_and_rejects_a_mismatch() {
    let router = EventRouter::default();
    let connection = ConnectionIdentity::new();
    let stream = router
        .prepare_run(connection.clone(), "run-1", "thread-1")
        .unwrap();

    notify(&router, &connection, "turn/started", started());

    assert_eq!(router.bind_turn(&stream, "turn-1"), Ok(()));
    assert_eq!(
        router.bind_turn(&stream, "turn-other"),
        Err(EventRouterError::TurnMismatch)
    );
    assert_eq!(stream.try_recv(), Some(event(1, RunEventKind::Running)));
}

#[test]
fn failed_and_interrupted_turns_drop_unbounded_runtime_details() {
    let router = EventRouter::default();
    let connection = ConnectionIdentity::new();

    for (index, status, kind) in [
        (1, "failed", RunEventKind::Failed),
        (2, "interrupted", RunEventKind::Interrupted),
    ] {
        let run_id = format!("run-{index}");
        let thread_id = format!("thread-{index}");
        let turn_id = format!("turn-{index}");
        let stream = router
            .subscribe_run(connection.clone(), &run_id, &thread_id, &turn_id)
            .unwrap();
        notify(
            &router,
            &connection,
            "turn/completed",
            json!({
                "threadId": thread_id,
                "turn": {
                    "id": turn_id,
                    "status": status,
                    "error": {"message": "SECRET".repeat(8 * 1024)},
                }
            }),
        );

        let event = stream.try_recv().unwrap();
        assert_eq!(
            event,
            RunEvent {
                run_id,
                sequence: 1,
                kind
            }
        );
        assert!(serialized_len(&event) <= MAX_RUN_EVENT_BYTES);
        assert!(!stream.is_active());
    }
}

#[test]
fn slow_consumers_get_an_explicit_gap_without_blocking_the_reader() {
    let router = EventRouter::default();
    let connection = ConnectionIdentity::new();
    let stream = router
        .subscribe_run(connection.clone(), "run-1", "thread-1", "turn-1")
        .unwrap();

    for index in 0..EVENT_QUEUE_CAPACITY + 4 {
        if index % 2 == 0 {
            notify(&router, &connection, "turn/started", started());
        } else {
            NotificationObserver::on_server_request(
                &router,
                &connection,
                &json!(index),
                "item/fileChange/requestApproval",
                &json!({"threadId": "thread-1", "turnId": "turn-1"}),
            );
        }
    }

    let events = drain(&stream);
    assert_eq!(events.len(), EVENT_QUEUE_CAPACITY);
    assert_eq!(
        events.last().map(|event| event.kind),
        Some(RunEventKind::Gap {
            reason: RunEventGapReason::ObserverLagged,
        })
    );
}

#[test]
fn event_count_single_event_and_total_bytes_are_hard_bounded() {
    let router = EventRouter::default();
    let connection = ConnectionIdentity::new();
    let run_id = "r".repeat(MAX_CORRELATION_ID_BYTES);
    let stream = router
        .subscribe_run(connection.clone(), &run_id, "thread-1", "turn-1")
        .unwrap();
    let mut events = Vec::new();

    for index in 0..MAX_RUN_EVENTS * 2 {
        if index % 2 == 0 {
            notify(&router, &connection, "turn/started", started());
        } else {
            NotificationObserver::on_server_request(
                &router,
                &connection,
                &json!(index),
                "item/permissions/requestApproval",
                &json!({"threadId": "thread-1", "turnId": "turn-1"}),
            );
        }
        if let Some(event) = stream.try_recv() {
            let is_gap = matches!(event.kind, RunEventKind::Gap { .. });
            events.push(event);
            if is_gap {
                break;
            }
        }
    }

    assert!(events.len() <= MAX_RUN_EVENTS);
    assert!(
        events
            .iter()
            .all(|event| serialized_len(event) <= MAX_RUN_EVENT_BYTES)
    );
    assert!(events.iter().map(serialized_len).sum::<usize>() <= MAX_RUN_EVENT_TOTAL_BYTES);
    assert_eq!(
        events.last().map(|event| event.kind),
        Some(RunEventKind::Gap {
            reason: RunEventGapReason::LimitExceeded,
        })
    );
}

#[test]
fn disconnect_deactivates_every_pending_run_without_inventing_a_terminal_event() {
    let router = EventRouter::default();
    let connection = ConnectionIdentity::new();
    let first = router
        .prepare_run(connection.clone(), "run-1", "thread-1")
        .unwrap();
    let second = router.prepare_run(connection, "run-2", "thread-2").unwrap();

    crate::app_server::process::ConnectionObserver::on_disconnected(&router);

    assert!(!first.is_active());
    assert!(!second.is_active());
    assert_eq!(drain(&first), vec![]);
    assert_eq!(drain(&second), vec![]);
}

#[test]
fn connection_forwards_approval_requests_before_its_safe_fallback_response() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let writer_writes = writes.clone();
    let connection = AppServerConnection::new(move |line| {
        writer_writes.lock().unwrap().push(line.to_string());
        Ok(())
    });
    let router = Arc::new(EventRouter::default());
    let stream = router
        .subscribe_run(
            connection.connection_identity(),
            "run-1",
            "thread-1",
            "turn-1",
        )
        .unwrap();
    connection.set_notification_observer(router);

    connection
        .handle_inbound(crate::app_server::wire::InboundMessage::ServerRequest {
            id: json!("approval-1"),
            method: "item/commandExecution/requestApproval".to_string(),
            params: json!({"threadId": "thread-1", "turnId": "turn-1"}),
        })
        .unwrap();

    assert_eq!(
        stream.try_recv(),
        Some(event(1, RunEventKind::WaitingApproval))
    );
    assert_eq!(
        writes
            .lock()
            .unwrap()
            .iter()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>(),
        vec![json!({
            "id": "approval-1",
            "error": {"code": -32601, "message": "Method not supported"},
        })]
    );
}

fn notify(router: &EventRouter, connection: &ConnectionIdentity, method: &str, params: Value) {
    NotificationObserver::on_notification(router, connection, method, &params);
}

fn started() -> Value {
    json!({"threadId": "thread-1", "turn": {"id": "turn-1", "status": "inProgress"}})
}

fn event(sequence: u32, kind: RunEventKind) -> RunEvent {
    RunEvent {
        run_id: "run-1".to_string(),
        sequence,
        kind,
    }
}

fn drain(stream: &RunEventStream) -> Vec<RunEvent> {
    std::iter::from_fn(|| stream.try_recv()).collect()
}
