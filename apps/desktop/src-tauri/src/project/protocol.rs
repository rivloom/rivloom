use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::types::{ProjectThread, ProjectThreadPage, ProjectThreadStatus};

pub(super) const MAX_CURSOR_BYTES: usize = 4 * 1024;
pub(super) const MAX_PAGE_THREADS: usize = 50;
pub(super) const MAX_THREAD_ID_BYTES: usize = 1024;
const MAX_THREAD_NAME_BYTES: usize = 1024;
const MAX_THREAD_PREVIEW_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ThreadProtocolError {
    InvalidResponse,
    CwdMismatch,
}

pub(super) fn list_params(cwd: &str, cursor: Option<&str>, limit: usize) -> Value {
    let mut params = Map::from_iter([
        ("cwd".to_string(), json!(cwd)),
        ("limit".to_string(), json!(limit)),
        ("sortKey".to_string(), json!("recency_at")),
        ("sortDirection".to_string(), json!("desc")),
    ]);
    if let Some(cursor) = cursor {
        params.insert("cursor".to_string(), json!(cursor));
    }
    Value::Object(params)
}

pub(super) fn start_params(cwd: &str) -> Value {
    json!({ "cwd": cwd })
}

pub(super) fn read_params(thread_id: &str) -> Value {
    json!({ "threadId": thread_id, "includeTurns": false })
}

pub(super) fn parse_list_response(
    response: Value,
    expected_cwd: &str,
    max_items: usize,
) -> Result<ProjectThreadPage, ThreadProtocolError> {
    let response: ThreadListWire =
        serde_json::from_value(response).map_err(|_| ThreadProtocolError::InvalidResponse)?;
    if response.data.len() > max_items
        || response
            .next_cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > MAX_CURSOR_BYTES)
    {
        return Err(ThreadProtocolError::InvalidResponse);
    }
    let data = response
        .data
        .into_iter()
        .map(|thread| normalize_thread(thread, expected_cwd))
        .collect::<Result<_, _>>()?;
    Ok(ProjectThreadPage {
        data,
        next_cursor: response.next_cursor,
    })
}

pub(super) fn parse_start_response(
    response: Value,
    expected_cwd: &str,
) -> Result<ProjectThread, ThreadProtocolError> {
    let response: ThreadStartWire =
        serde_json::from_value(response).map_err(|_| ThreadProtocolError::InvalidResponse)?;
    if response.cwd != expected_cwd {
        return Err(ThreadProtocolError::CwdMismatch);
    }
    normalize_thread(response.thread, expected_cwd)
}

pub(super) fn parse_read_response(
    response: Value,
    expected_cwd: &str,
) -> Result<ProjectThread, ThreadProtocolError> {
    let response: ThreadReadWire =
        serde_json::from_value(response).map_err(|_| ThreadProtocolError::InvalidResponse)?;
    normalize_thread(response.thread, expected_cwd)
}

fn normalize_thread(
    thread: ThreadWire,
    expected_cwd: &str,
) -> Result<ProjectThread, ThreadProtocolError> {
    if thread.cwd != expected_cwd {
        return Err(ThreadProtocolError::CwdMismatch);
    }
    if thread.id.is_empty() || thread.id.len() > MAX_THREAD_ID_BYTES {
        return Err(ThreadProtocolError::InvalidResponse);
    }
    Ok(ProjectThread {
        id: thread.id,
        name: thread
            .name
            .map(|name| truncate_utf8(name, MAX_THREAD_NAME_BYTES)),
        preview: truncate_utf8(thread.preview, MAX_THREAD_PREVIEW_BYTES),
        created_at: thread.created_at,
        updated_at: thread.updated_at,
        recency_at: thread.recency_at,
        status: thread.status.into(),
        cwd: thread.cwd,
    })
}

fn truncate_utf8(mut value: String, max_bytes: usize) -> String {
    if value.len() > max_bytes {
        let mut end = max_bytes;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
    value
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadListWire {
    data: Vec<ThreadWire>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadStartWire {
    thread: ThreadWire,
    cwd: String,
}

#[derive(Deserialize)]
struct ThreadReadWire {
    thread: ThreadWire,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadWire {
    id: String,
    name: Option<String>,
    preview: String,
    created_at: i64,
    updated_at: i64,
    recency_at: Option<i64>,
    status: ThreadStatusWire,
    cwd: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
enum ThreadStatusWire {
    NotLoaded,
    Idle,
    SystemError,
    Active,
}

impl From<ThreadStatusWire> for ProjectThreadStatus {
    fn from(status: ThreadStatusWire) -> Self {
        match status {
            ThreadStatusWire::NotLoaded => Self::NotLoaded,
            ThreadStatusWire::Idle => Self::Idle,
            ThreadStatusWire::SystemError => Self::SystemError,
            ThreadStatusWire::Active => Self::Active,
        }
    }
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
