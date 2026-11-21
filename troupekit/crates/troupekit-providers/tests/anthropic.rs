mod common;

use common::*;
use serde_json::json;
use troupekit_core::error::ProviderErrorKind;
use troupekit_core::message::{Message, Role};
use troupekit_providers::request::{CompletionRequest, Delta, StopReason};
use troupekit_providers::wire::{decode_body, encode, Decoder, Protocol, Quirks, SchemaMode};

fn anthropic_quirks() -> Quirks {
    Quirks {
        temperature: false,
        ..Quirks::default()
    }
}

#[test]
fn encodes_the_golden_request() {
    let body = encode(
        Protocol::Anthropic,
        &anthropic_quirks(),
        "claude-opus-5-5",
        &rich_request(),
        false,
    )
    .unwrap();
    assert_eq!(body, golden("anthropic_request.json"));
}

#[test]
fn stream_flag_temperature_and_schema_follow_mode_and_quirks() {
    let req = rich_request();
    let body = encode(Protocol::Anthropic, &Quirks::default(), "m", &req, true).unwrap();
    assert_eq!(body["stream"], json!(true));
    assert_eq!(body["temperature"], json!(0.5));
    let plain = Quirks {
        output_schema: SchemaMode::Unsupported,
        ..Quirks::default()
    };
    assert!(encode(Protocol::Anthropic, &plain, "m", &req, false)
        .unwrap()
        .get("output_config")
        .is_none());
}

#[test]
fn merges_consecutive_tool_results_into_one_user_turn() {
    let mut req = CompletionRequest::user("go", 10);
    req.messages
        .push(Message::new(Role::Assistant, vec![call("a", "t"), call("b", "t")]).unwrap());
    req.messages
        .push(Message::new(Role::Tool, vec![result("a")]).unwrap());
    req.messages
        .push(Message::new(Role::Tool, vec![result("b")]).unwrap());
    let body = encode(Protocol::Anthropic, &anthropic_quirks(), "m", &req, false).unwrap();
    let turns = body["messages"].as_array().unwrap();
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[2]["role"], "user");
    assert_eq!(turns[2]["content"].as_array().unwrap().len(), 2);
}

#[test]
fn unary_and_stream_fold_to_the_same_reply() {
    let unary =
        fold_all(&decode_body(Protocol::Anthropic, &golden("anthropic_reply.json")).unwrap());
    let (deltas, streamed) =
        stream_deltas(Protocol::Anthropic, &golden_text("anthropic_stream.sse"));
    assert_eq!(unary, expected_reply("toolu_1"));
    assert_eq!(streamed, unary);
    assert!(deltas.contains(&Delta::TextDelta {
        text: "Mon: run ".into()
    }));
}

#[test]
fn maps_stream_error_events() {
    let mut decoder = Decoder::new(Protocol::Anthropic);
    let err = decoder
        .event(&json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}))
        .unwrap_err();
    assert_eq!(
        (err.kind, err.message.as_str()),
        (ProviderErrorKind::Server, "Overloaded")
    );
    let err = decoder
        .event(&json!({"type":"error","error":{"type":"rate_limit_error","message":"slow"}}))
        .unwrap_err();
    assert_eq!(err.kind, ProviderErrorKind::RateLimited);
    let err = decode_body(
        Protocol::Anthropic,
        &json!({"type":"error","error":{"type":"invalid_request_error","message":"bad"}}),
    );
    assert_eq!(err.unwrap_err().kind, ProviderErrorKind::BadRequest);
}

#[test]
fn maps_stop_reasons() {
    let table = [
        ("end_turn", StopReason::EndTurn),
        ("stop_sequence", StopReason::EndTurn),
        ("max_tokens", StopReason::MaxTokens),
        ("refusal", StopReason::Refusal),
        ("pause_turn", StopReason::Other("pause_turn".into())),
    ];
    for (wire, want) in table {
        let body = json!({"type":"message","content":[],"stop_reason":wire,"usage":{"input_tokens":1,"output_tokens":1}});
        assert_eq!(
            fold_all(&decode_body(Protocol::Anthropic, &body).unwrap()).stop,
            want,
            "{wire}"
        );
    }
}
