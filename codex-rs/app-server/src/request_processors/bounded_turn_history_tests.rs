use super::*;
use codex_app_server_protocol::TurnError;
use codex_app_server_protocol::TurnStatus;
use pretty_assertions::assert_eq;

#[test]
fn bounds_multibyte_summary_and_reports_turn_id() {
    let turn = Turn {
        id: "turn-1".to_string(),
        items: vec![
            ThreadItem::UserMessage {
                id: "user-1".to_string(),
                client_id: None,
                content: vec![UserInput::Text {
                    text: "界".repeat(MAX_TURN_BYTES),
                    text_elements: Vec::new(),
                }],
            },
            ThreadItem::AgentMessage {
                id: "agent-1".to_string(),
                text: "答".repeat(MAX_TURN_BYTES),
                phase: None,
                memory_citation: None,
                delivery: None,
            },
        ],
        items_view: TurnItemsView::Summary,
        status: TurnStatus::Completed,
        error: Some(TurnError {
            message: "错".repeat(MAX_ERROR_BYTES),
            codex_error_info: None,
            additional_details: None,
        }),
        started_at: None,
        completed_at: None,
        duration_ms: None,
    };
    let response = ThreadTurnsListResponse {
        data: vec![turn],
        next_cursor: Some("next".to_string()),
        backwards_cursor: Some("back".to_string()),
        truncated_turn_ids: Vec::new(),
    };
    let budget = turn_history_budget(
        /*page_size*/ 20,
        Some(MIN_RESULT_BYTES as u32),
        TurnItemsView::Summary,
    )
    .expect("valid budget")
    .expect("budget should be present");

    let bounded = finalize_turn_history_response(response, Some(budget)).expect("bounded response");

    assert_eq!(bounded.truncated_turn_ids, vec!["turn-1"]);
    assert!(serde_json::to_vec(&bounded).expect("serialize").len() <= MIN_RESULT_BYTES);
    assert!(
        serde_json::to_vec(&bounded.data[0])
            .expect("serialize")
            .len()
            <= MAX_TURN_BYTES
    );
}

#[test]
fn budget_caps_page_size_and_rejects_full_view() {
    let minimum = turn_history_budget(
        /*page_size*/ 20,
        Some(MIN_RESULT_BYTES as u32),
        TurnItemsView::Summary,
    )
    .expect("valid budget")
    .expect("budget should be present");
    assert_eq!(minimum.page_size, 1);

    let desktop = turn_history_budget(
        /*page_size*/ 20,
        Some(MAX_RESULT_BYTES as u32),
        TurnItemsView::Summary,
    )
    .expect("valid budget")
    .expect("budget should be present");
    assert_eq!(desktop.page_size, 20);

    assert!(
        turn_history_budget(
            /*page_size*/ 1,
            Some(MIN_RESULT_BYTES as u32),
            TurnItemsView::Full,
        )
        .is_err()
    );
}

#[test]
fn omitting_budget_preserves_response() {
    let response = ThreadTurnsListResponse {
        data: Vec::new(),
        next_cursor: None,
        backwards_cursor: None,
        truncated_turn_ids: Vec::new(),
    };
    assert_eq!(
        finalize_turn_history_response(response.clone(), /*budget*/ None)
            .expect("unbounded response"),
        response
    );
}
