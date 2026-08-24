use pretty_assertions::assert_eq;
use serde_json::json;

use super::InboundMessage;
use super::JsonLineDecoder;
use super::MAX_JSON_LINE_BYTES;
use super::parse_inbound_message;

#[test]
fn parses_result_response() {
    let message = parse_inbound_message(r#"{"id":7,"result":{"account":null}}"#).unwrap();

    assert_eq!(
        message,
        InboundMessage::Response {
            id: 7,
            result: json!({"account": null}),
        }
    );
}

#[test]
fn parses_error_response() {
    let message =
        parse_inbound_message(r#"{"id":8,"error":{"code":-32600,"message":"invalid request"}}"#)
            .unwrap();

    assert_eq!(
        message,
        InboundMessage::ResponseError {
            id: 8,
            code: -32600,
            message: "invalid request".to_string(),
        }
    );
}

#[test]
fn parses_notification() {
    let message =
        parse_inbound_message(r#"{"method":"account/updated","params":{"authMode":"chatgpt"}}"#)
            .unwrap();

    assert_eq!(
        message,
        InboundMessage::Notification {
            method: "account/updated".to_string(),
            params: json!({"authMode": "chatgpt"}),
        }
    );
}

#[test]
fn parses_server_request_and_preserves_its_id() {
    let message = parse_inbound_message(
        r#"{"method":"item/commandExecution/requestApproval","id":"approval-1","params":{"command":"cargo test"}}"#,
    )
    .unwrap();

    assert_eq!(
        message,
        InboundMessage::ServerRequest {
            id: json!("approval-1"),
            method: "item/commandExecution/requestApproval".to_string(),
            params: json!({"command": "cargo test"}),
        }
    );
}

#[test]
fn rejects_invalid_message_shape_without_exposing_payloads() {
    let error = parse_inbound_message(
        r#"{"id":9,"result":{"token":"TOP_SECRET"},"error":{"code":1,"message":"bad"}}"#,
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "App Server response must contain exactly one result or error"
    );
    assert!(!error.to_string().contains("TOP_SECRET"));
}

#[test]
fn rejects_non_integer_client_response_id() {
    let error = parse_inbound_message(r#"{"id":"client-7","result":{}}"#).unwrap_err();

    assert_eq!(
        error.to_string(),
        "App Server response ID must be a non-negative integer"
    );
}

#[test]
fn decoder_joins_split_chunks() {
    let mut decoder = JsonLineDecoder::new(MAX_JSON_LINE_BYTES);

    assert_eq!(
        decoder.push(br#"{"id":7,"res"#).unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(
        decoder.push(b"ult\":{}}\n").unwrap(),
        vec![r#"{"id":7,"result":{}}"#.to_string()]
    );
}

#[test]
fn decoder_returns_multiple_lines_from_one_chunk() {
    let mut decoder = JsonLineDecoder::new(MAX_JSON_LINE_BYTES);

    assert_eq!(
        decoder
            .push(b"{\"id\":1,\"result\":{}}\n{\"id\":2,\"result\":{}}\n")
            .unwrap(),
        vec![
            r#"{"id":1,"result":{}}"#.to_string(),
            r#"{"id":2,"result":{}}"#.to_string(),
        ]
    );
}

#[test]
fn decoder_removes_carriage_return_from_crlf() {
    let mut decoder = JsonLineDecoder::new(MAX_JSON_LINE_BYTES);

    assert_eq!(
        decoder.push(b"{\"id\":1,\"result\":{}}\r\n").unwrap(),
        vec![r#"{"id":1,"result":{}}"#.to_string()]
    );
}

#[test]
fn decoder_rejects_invalid_utf8() {
    let mut decoder = JsonLineDecoder::new(MAX_JSON_LINE_BYTES);

    let error = decoder.push(&[0xff, b'\n']).unwrap_err();

    assert_eq!(error.to_string(), "App Server emitted invalid UTF-8");
}

#[test]
fn decoder_rejects_an_over_limit_buffer_and_clears_it() {
    let mut decoder = JsonLineDecoder::new(/*max_line_bytes*/ 8);

    let error = decoder.push(b"123456789").unwrap_err();

    assert_eq!(
        error.to_string(),
        "App Server JSONL message exceeded the 8-byte limit"
    );
    assert_eq!(decoder.push(b"{}\n").unwrap(), vec!["{}".to_string()]);
}
