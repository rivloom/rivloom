use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::{
    MAX_CURSOR_BYTES, MAX_PAGE_THREADS, ThreadProtocolError, parse_list_response,
    parse_read_response, parse_start_response,
};

const CWD: &str = "C:/work/project";

#[test]
fn list_parser_returns_only_bounded_whitelisted_fields() {
    let response = json!({
        "data": [{
            "id": "thr-1",
            "name": "Planning",
            "preview": "First prompt",
            "createdAt": 10,
            "updatedAt": 20,
            "recencyAt": 30,
            "status": { "type": "active", "activeFlags": ["waitingOnApproval"] },
            "cwd": CWD,
            "turns": [{ "private": "discarded" }],
            "path": "C:/private/rollout.jsonl",
            "projectId": "experimental",
            "unknown": "discarded"
        }],
        "nextCursor": "opaque-next",
        "backwardsCursor": "discarded"
    });

    let page = parse_list_response(response, CWD, MAX_PAGE_THREADS).unwrap();

    assert_eq!(
        serde_json::to_value(page).unwrap(),
        json!({
            "data": [{
                "id": "thr-1",
                "name": "Planning",
                "preview": "First prompt",
                "createdAt": 10,
                "updatedAt": 20,
                "recencyAt": 30,
                "status": "active",
                "cwd": CWD
            }],
            "nextCursor": "opaque-next"
        })
    );
}

#[test]
fn parser_rejects_missing_malformed_and_oversized_fields() {
    let mut missing_id = valid_thread();
    missing_id.as_object_mut().unwrap().remove("id");
    let cases = [
        missing_id,
        with_field("createdAt", json!("not-a-timestamp")),
    ];

    for thread in cases {
        assert_eq!(
            parse_read_response(json!({ "thread": thread }), CWD),
            Err(ThreadProtocolError::InvalidResponse)
        );
    }

    assert_eq!(
        parse_list_response(
            json!({ "data": vec![valid_thread(); MAX_PAGE_THREADS + 1] }),
            CWD,
            MAX_PAGE_THREADS,
        ),
        Err(ThreadProtocolError::InvalidResponse)
    );
    assert_eq!(
        parse_list_response(
            json!({ "data": [], "nextCursor": "x".repeat(MAX_CURSOR_BYTES + 1) }),
            CWD,
            MAX_PAGE_THREADS,
        ),
        Err(ThreadProtocolError::InvalidResponse)
    );
}

#[test]
fn long_display_fields_are_utf8_safely_bounded_without_rejecting_the_page() {
    let thread = with_fields([
        ("preview", json!("界".repeat(16 * 1024))),
        ("name", json!("界".repeat(1024))),
    ]);

    let parsed = parse_read_response(json!({ "thread": thread }), CWD).unwrap();

    assert_eq!(parsed.preview.len(), 16 * 1024 - 1);
    assert_eq!(parsed.name.unwrap().len(), 1023);
}

#[test]
fn every_response_form_rejects_a_cwd_mismatch() {
    let thread = with_field("cwd", json!("C:/other"));

    assert_eq!(
        parse_list_response(json!({ "data": [thread.clone()] }), CWD, MAX_PAGE_THREADS,),
        Err(ThreadProtocolError::CwdMismatch)
    );
    assert_eq!(
        parse_start_response(json!({ "thread": valid_thread(), "cwd": "C:/other" }), CWD),
        Err(ThreadProtocolError::CwdMismatch)
    );
    assert_eq!(
        parse_start_response(json!({ "thread": thread, "cwd": CWD }), CWD),
        Err(ThreadProtocolError::CwdMismatch)
    );
    assert_eq!(
        parse_read_response(
            json!({ "thread": with_field("cwd", json!("C:/other")) }),
            CWD,
        ),
        Err(ThreadProtocolError::CwdMismatch)
    );
}

fn valid_thread() -> Value {
    json!({
        "id": "thr-1",
        "name": null,
        "preview": "Preview",
        "createdAt": 10,
        "updatedAt": 20,
        "recencyAt": null,
        "status": { "type": "idle" },
        "cwd": CWD
    })
}

fn with_field(field: &str, value: Value) -> Value {
    with_fields([(field, value)])
}

fn with_fields<const N: usize>(fields: [(&str, Value); N]) -> Value {
    let mut thread = valid_thread();
    for (field, value) in fields {
        thread[field] = value;
    }
    thread
}
