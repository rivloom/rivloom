use crate::error_code::internal_error;
use crate::error_code::invalid_request;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadTurnsListResponse;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnItemsView;
use codex_app_server_protocol::UserInput;

const MIN_RESULT_BYTES: usize = 256 * 1024;
const MAX_RESULT_BYTES: usize = 3 * 1024 * 1024;
const MAX_TURN_BYTES: usize = 128 * 1024;
const MAX_TURN_RESULT_BYTES: usize = MAX_TURN_BYTES + MAX_ID_BYTES + 1024;
const RESPONSE_OVERHEAD_BYTES: usize = 16 * 1024;
const MAX_ID_BYTES: usize = 1024;
const MAX_ERROR_BYTES: usize = 8 * 1024;
const INITIAL_MESSAGE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy)]
pub(super) struct TurnHistoryBudget {
    max_bytes: usize,
    pub(super) page_size: usize,
}

pub(super) fn turn_history_budget(
    page_size: usize,
    max_bytes: Option<u32>,
    items_view: TurnItemsView,
) -> Result<Option<TurnHistoryBudget>, JSONRPCErrorError> {
    let Some(max_bytes) = max_bytes.map(|value| value as usize) else {
        return Ok(None);
    };
    if matches!(items_view, TurnItemsView::Full) {
        return Err(invalid_request(
            "thread/turns/list maxBytes does not support itemsView full",
        ));
    }
    if max_bytes < MIN_RESULT_BYTES {
        return Err(invalid_request(format!(
            "thread/turns/list maxBytes must be at least {MIN_RESULT_BYTES}"
        )));
    }

    let max_bytes = max_bytes.min(MAX_RESULT_BYTES);
    let page_capacity = (max_bytes - RESPONSE_OVERHEAD_BYTES) / MAX_TURN_RESULT_BYTES;
    Ok(Some(TurnHistoryBudget {
        max_bytes,
        page_size: page_size.min(page_capacity.max(/*other*/ 1)),
    }))
}

pub(super) fn finalize_turn_history_response(
    mut response: ThreadTurnsListResponse,
    budget: Option<TurnHistoryBudget>,
) -> Result<ThreadTurnsListResponse, JSONRPCErrorError> {
    let Some(budget) = budget else {
        return Ok(response);
    };

    for turn in &mut response.data {
        if turn.id.len() > MAX_ID_BYTES {
            return Err(internal_error(
                "stored turn id exceeds the bounded history limit",
            ));
        }
        if bound_turn(turn)? {
            response.truncated_turn_ids.push(turn.id.clone());
        }
    }

    let serialized_len = serde_json::to_vec(&response)
        .map_err(|err| internal_error(format!("failed to serialize bounded history: {err}")))?
        .len();
    if serialized_len > budget.max_bytes {
        return Err(internal_error(
            "bounded thread history exceeded its serialized byte budget",
        ));
    }
    Ok(response)
}

fn bound_turn(turn: &mut Turn) -> Result<bool, JSONRPCErrorError> {
    let mut truncated = truncate_error(turn);
    if serialized_len(turn)? <= MAX_TURN_BYTES {
        return Ok(truncated);
    }

    truncated = true;
    let mut bounded_items = Vec::with_capacity(turn.items.len().min(/*other*/ 2));
    for item in turn.items.drain(..) {
        match item {
            ThreadItem::UserMessage {
                mut id,
                mut client_id,
                content,
            } => {
                truncate_utf8(&mut id, MAX_ID_BYTES);
                if let Some(client_id) = client_id.as_mut() {
                    truncate_utf8(client_id, MAX_ID_BYTES);
                }
                let content = content.into_iter().find_map(|input| match input {
                    UserInput::Text { mut text, .. } => {
                        truncate_utf8(&mut text, INITIAL_MESSAGE_BYTES);
                        Some(UserInput::Text {
                            text,
                            text_elements: Vec::new(),
                        })
                    }
                    UserInput::Image { .. }
                    | UserInput::LocalImage { .. }
                    | UserInput::Audio { .. }
                    | UserInput::LocalAudio { .. }
                    | UserInput::Skill { .. }
                    | UserInput::Mention { .. } => None,
                });
                bounded_items.push(ThreadItem::UserMessage {
                    id,
                    client_id,
                    content: content.into_iter().collect(),
                });
            }
            ThreadItem::AgentMessage {
                mut id,
                mut text,
                phase,
                ..
            } => {
                truncate_utf8(&mut id, MAX_ID_BYTES);
                truncate_utf8(&mut text, INITIAL_MESSAGE_BYTES);
                bounded_items.push(ThreadItem::AgentMessage {
                    id,
                    text,
                    phase,
                    memory_citation: None,
                    delivery: None,
                });
            }
            ThreadItem::HookPrompt { .. }
            | ThreadItem::Plan { .. }
            | ThreadItem::Reasoning { .. }
            | ThreadItem::CommandExecution { .. }
            | ThreadItem::FileChange { .. }
            | ThreadItem::McpToolCall { .. }
            | ThreadItem::DynamicToolCall { .. }
            | ThreadItem::CollabAgentToolCall { .. }
            | ThreadItem::SubAgentActivity { .. }
            | ThreadItem::WebSearch(_)
            | ThreadItem::ImageView { .. }
            | ThreadItem::Sleep(_)
            | ThreadItem::ImageGeneration(_)
            | ThreadItem::EnteredReviewMode { .. }
            | ThreadItem::ExitedReviewMode { .. }
            | ThreadItem::ContextCompaction { .. } => {}
        }
    }
    turn.items = bounded_items;

    while serialized_len(turn)? > MAX_TURN_BYTES {
        if !halve_display_strings(turn) {
            turn.items.clear();
            turn.error = None;
            break;
        }
    }
    if serialized_len(turn)? > MAX_TURN_BYTES {
        return Err(internal_error(
            "failed to truncate a turn to the bounded history limit",
        ));
    }
    Ok(truncated)
}

fn truncate_error(turn: &mut Turn) -> bool {
    let Some(error) = turn.error.as_mut() else {
        return false;
    };
    let mut truncated = truncate_utf8(&mut error.message, MAX_ERROR_BYTES);
    if let Some(details) = error.additional_details.as_mut() {
        truncated |= truncate_utf8(details, MAX_ERROR_BYTES);
    }
    truncated
}

fn halve_display_strings(turn: &mut Turn) -> bool {
    let mut changed = false;
    for item in &mut turn.items {
        match item {
            ThreadItem::UserMessage { content, .. } => {
                for input in content {
                    if let UserInput::Text { text, .. } = input {
                        changed |= truncate_utf8(text, text.len() / 2);
                    }
                }
            }
            ThreadItem::AgentMessage { text, .. } => {
                changed |= truncate_utf8(text, text.len() / 2);
            }
            _ => {}
        }
    }
    if let Some(error) = turn.error.as_mut() {
        let message_bytes = error.message.len() / 2;
        changed |= truncate_utf8(&mut error.message, message_bytes);
        if let Some(details) = error.additional_details.as_mut() {
            let details_bytes = details.len() / 2;
            changed |= truncate_utf8(details, details_bytes);
        }
    }
    changed
}

fn serialized_len(turn: &Turn) -> Result<usize, JSONRPCErrorError> {
    serde_json::to_vec(turn)
        .map(|value| value.len())
        .map_err(|err| internal_error(format!("failed to serialize bounded turn: {err}")))
}

fn truncate_utf8(value: &mut String, max_bytes: usize) -> bool {
    if value.len() <= max_bytes {
        return false;
    }
    let mut boundary = max_bytes.min(value.len());
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    true
}

#[cfg(test)]
#[path = "bounded_turn_history_tests.rs"]
mod tests;
