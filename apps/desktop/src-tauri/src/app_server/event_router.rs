use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::Weak;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use super::ConnectionIdentity;
use super::NotificationObserver;

const MAX_CORRELATION_ID_BYTES: usize = 128;
const MAX_ACTIVE_ROUTES: usize = 32;
const EVENT_QUEUE_CAPACITY: usize = 8;
pub(crate) const MAX_RUN_EVENTS: usize = 128;
pub(crate) const MAX_RUN_EVENT_BYTES: usize = 512;
pub(crate) const MAX_RUN_EVENT_TOTAL_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RunEventGapReason {
    ObserverLagged,
    LimitExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum RunEventKind {
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Interrupted,
    Gap { reason: RunEventGapReason },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunEvent {
    pub(crate) run_id: String,
    pub(crate) sequence: u32,
    pub(crate) kind: RunEventKind,
}

pub(crate) struct RunEventStream {
    router_identity: Arc<()>,
    route_id: u64,
    queue: Arc<EventQueue>,
}

impl RunEventStream {
    pub(crate) fn try_recv(&self) -> Option<RunEvent> {
        self.queue
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }

    pub(crate) fn is_active(&self) -> bool {
        self.queue.active.load(Ordering::Acquire)
    }
}

#[derive(Default)]
pub(crate) struct EventRouter {
    identity: Arc<()>,
    next_route_id: AtomicU64,
    routes: Mutex<Vec<Route>>,
}

impl EventRouter {
    pub(crate) fn prepare_run(
        &self,
        connection_identity: ConnectionIdentity,
        run_id: &str,
        thread_id: &str,
    ) -> Result<RunEventStream, EventRouterError> {
        if !valid_id(run_id) || !valid_id(thread_id) {
            return Err(EventRouterError::InvalidCorrelation);
        }
        let mut routes = self.routes.lock().unwrap_or_else(PoisonError::into_inner);
        routes.retain(|route| route.queue.strong_count() > 0);
        if routes.len() >= MAX_ACTIVE_ROUTES {
            return Err(EventRouterError::TooManyRoutes);
        }
        if routes.iter().any(|route| {
            route.connection_identity == connection_identity
                && (route.run_id == run_id || route.thread_id == thread_id)
        }) {
            return Err(EventRouterError::DuplicateRoute);
        }
        let route_id = self.next_route_id.fetch_add(1, Ordering::Relaxed);
        let queue = Arc::new(EventQueue::default());
        routes.push(Route {
            route_id,
            connection_identity,
            run_id: run_id.to_string(),
            thread_id: thread_id.to_string(),
            turn_id: None,
            next_sequence: 1,
            total_bytes: 0,
            saturated: false,
            queue: Arc::downgrade(&queue),
        });
        Ok(RunEventStream {
            router_identity: self.identity.clone(),
            route_id,
            queue,
        })
    }

    pub(crate) fn subscribe_run(
        &self,
        connection_identity: ConnectionIdentity,
        run_id: &str,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<RunEventStream, EventRouterError> {
        let stream = self.prepare_run(connection_identity, run_id, thread_id)?;
        self.bind_turn(&stream, turn_id)?;
        Ok(stream)
    }

    pub(crate) fn bind_turn(
        &self,
        stream: &RunEventStream,
        turn_id: &str,
    ) -> Result<(), EventRouterError> {
        if !Arc::ptr_eq(&self.identity, &stream.router_identity) || !valid_id(turn_id) {
            return Err(EventRouterError::InvalidCorrelation);
        }
        let mut routes = self.routes.lock().unwrap_or_else(PoisonError::into_inner);
        let route = routes
            .iter_mut()
            .find(|route| route.route_id == stream.route_id)
            .ok_or(EventRouterError::UnknownRoute)?;
        match &route.turn_id {
            Some(expected) if expected != turn_id => Err(EventRouterError::TurnMismatch),
            Some(_) => Ok(()),
            None => {
                route.turn_id = Some(turn_id.to_string());
                Ok(())
            }
        }
    }

    fn route(&self, connection_identity: &ConnectionIdentity, method: &str, params: &Value) {
        let Some(normalized) = normalize_event(method, params) else {
            return;
        };
        let mut routes = self.routes.lock().unwrap_or_else(PoisonError::into_inner);
        routes.retain(|route| route.queue.strong_count() > 0);
        let Some(index) = routes.iter().position(|route| {
            route.connection_identity == *connection_identity
                && route.thread_id == normalized.thread_id
                && route
                    .turn_id
                    .as_deref()
                    .is_none_or(|turn_id| turn_id == normalized.turn_id)
        }) else {
            return;
        };
        let route = &mut routes[index];
        if route.turn_id.is_none() && normalized.kind != RunEventKind::Running {
            return;
        }
        route
            .turn_id
            .get_or_insert_with(|| normalized.turn_id.to_string());
        route.push(normalized.kind);
        if normalized.terminal {
            if let Some(queue) = route.queue.upgrade() {
                queue.active.store(false, Ordering::Release);
            }
            routes.remove(index);
        }
    }
}

impl NotificationObserver for EventRouter {
    fn on_notification(
        &self,
        connection_identity: &ConnectionIdentity,
        method: &str,
        params: &Value,
    ) {
        self.route(connection_identity, method, params);
    }

    fn on_server_request(
        &self,
        connection_identity: &ConnectionIdentity,
        _request_id: &Value,
        method: &str,
        params: &Value,
    ) {
        self.route(connection_identity, method, params);
    }
}

struct Route {
    route_id: u64,
    connection_identity: ConnectionIdentity,
    run_id: String,
    thread_id: String,
    turn_id: Option<String>,
    next_sequence: u32,
    total_bytes: usize,
    saturated: bool,
    queue: Weak<EventQueue>,
}

impl Route {
    fn push(&mut self, kind: RunEventKind) {
        if self.saturated {
            return;
        }
        let event = self.event(kind);
        let gap = self.event(RunEventKind::Gap {
            reason: RunEventGapReason::LimitExceeded,
        });
        let event_bytes = serialized_len(&event);
        let gap_bytes = serialized_len(&gap);
        if self.next_sequence as usize >= MAX_RUN_EVENTS
            || event_bytes > MAX_RUN_EVENT_BYTES
            || self.total_bytes + event_bytes + gap_bytes > MAX_RUN_EVENT_TOTAL_BYTES
        {
            self.saturated = true;
            self.push_to_queue(gap);
            return;
        }
        self.next_sequence += 1;
        self.total_bytes += event_bytes;
        self.push_to_queue(event);
    }

    fn event(&self, kind: RunEventKind) -> RunEvent {
        RunEvent {
            run_id: self.run_id.clone(),
            sequence: self.next_sequence,
            kind,
        }
    }

    fn push_to_queue(&self, event: RunEvent) {
        if let Some(queue) = self.queue.upgrade() {
            queue.push(event);
        }
    }
}

struct NormalizedEvent<'a> {
    thread_id: &'a str,
    turn_id: &'a str,
    kind: RunEventKind,
    terminal: bool,
}

fn normalize_event<'a>(method: &str, params: &'a Value) -> Option<NormalizedEvent<'a>> {
    let thread_id = valid_value_id(params.get("threadId")?)?;
    let (turn_id, kind, terminal) = match method {
        "turn/started" => {
            let turn = params.get("turn")?;
            if turn.get("status")?.as_str()? != "inProgress" {
                return None;
            }
            (
                valid_value_id(turn.get("id")?)?,
                RunEventKind::Running,
                false,
            )
        }
        "turn/completed" => {
            let turn = params.get("turn")?;
            let kind = match turn.get("status")?.as_str()? {
                "completed" => RunEventKind::Completed,
                "failed" => RunEventKind::Failed,
                "interrupted" => RunEventKind::Interrupted,
                _ => return None,
            };
            (valid_value_id(turn.get("id")?)?, kind, true)
        }
        "item/commandExecution/requestApproval"
        | "item/fileChange/requestApproval"
        | "item/permissions/requestApproval" => (
            valid_value_id(params.get("turnId")?)?,
            RunEventKind::WaitingApproval,
            false,
        ),
        _ => return None,
    };
    Some(NormalizedEvent {
        thread_id,
        turn_id,
        kind,
        terminal,
    })
}

fn valid_value_id(value: &Value) -> Option<&str> {
    value.as_str().filter(|value| valid_id(value))
}

fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_CORRELATION_ID_BYTES
}

fn serialized_len(event: &RunEvent) -> usize {
    serde_json::to_vec(event).map_or(usize::MAX, |bytes| bytes.len())
}

struct EventQueue {
    active: AtomicBool,
    events: Mutex<VecDeque<RunEvent>>,
}

impl Default for EventQueue {
    fn default() -> Self {
        Self {
            active: AtomicBool::new(true),
            events: Mutex::default(),
        }
    }
}

impl EventQueue {
    fn push(&self, event: RunEvent) {
        let mut events = self.events.lock().unwrap_or_else(PoisonError::into_inner);
        if events.len() < EVENT_QUEUE_CAPACITY {
            events.push_back(event);
            return;
        }
        if matches!(
            events.back().map(|event| event.kind),
            Some(RunEventKind::Gap { .. })
        ) {
            return;
        }
        events.pop_back();
        events.push_back(RunEvent {
            run_id: event.run_id,
            sequence: event.sequence,
            kind: RunEventKind::Gap {
                reason: RunEventGapReason::ObserverLagged,
            },
        });
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum EventRouterError {
    #[error("run event correlation is invalid")]
    InvalidCorrelation,
    #[error("too many run event routes are active")]
    TooManyRoutes,
    #[error("run event route already exists")]
    DuplicateRoute,
    #[error("run event route no longer exists")]
    UnknownRoute,
    #[error("Codex turn does not match the prepared run")]
    TurnMismatch,
}

#[cfg(test)]
#[path = "event_router_tests.rs"]
mod tests;
