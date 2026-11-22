use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use troupekit_core::http::{ChunkBody, HttpError, HttpRequest, HttpResponse, Transport};

/// What a fake transport answers: status, content type and body.
/// Built by the `sse_*` and `json_reply` helpers.
/// The body is sent as one chunk.
pub struct Reply {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

type Handler = dyn Fn(&HttpRequest) -> Reply + Send + Sync;

/// A Troupekit transport that answers every request with a pure function
/// of it and records each request, so tests can check headers (the key
/// reached `x-api-key`) without a network.
pub struct FnTransport {
    handler: Box<Handler>,
    sent: Mutex<Vec<HttpRequest>>,
}

impl FnTransport {
    /// Wraps `handler` as a transport.
    /// Requests are recorded in order.
    /// Returns it in an `Arc`, ready for `Deps`.
    pub fn new(handler: impl Fn(&HttpRequest) -> Reply + Send + Sync + 'static) -> Arc<FnTransport> {
        Arc::new(FnTransport { handler: Box::new(handler), sent: Mutex::new(Vec::new()) })
    }

    /// Returns every request sent so far, oldest first.
    /// Bodies included.
    /// Cloned out of the lock.
    pub fn sent(&self) -> Vec<HttpRequest> {
        self.sent.lock().expect("fake transport lock").clone()
    }
}

#[async_trait]
impl Transport for FnTransport {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let reply = (self.handler)(&req);
        self.sent.lock().expect("fake transport lock").push(req.clone());
        Ok(HttpResponse {
            status: reply.status,
            url: req.url.clone(),
            headers: vec![("content-type".into(), reply.content_type.into())],
            body: Box::new(ChunkBody::new(vec![reply.body.into_bytes()])),
            truncated: false,
        })
    }
}

fn sse(events: Vec<Value>) -> String {
    events
        .into_iter()
        .map(|e| format!("event: {}\ndata: {}\n\n", e["type"].as_str().expect("typed event"), e))
        .collect()
}

fn start_and_stop(blocks: Vec<Value>, stop: &str) -> Reply {
    let mut events = vec![json!({"type": "message_start", "message": {"id": "msg_1", "type": "message", "role": "assistant", "content": [], "usage": {"input_tokens": 100, "output_tokens": 1}}})];
    events.extend(blocks);
    events.push(json!({"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": null}, "usage": {"output_tokens": 50}}));
    events.push(json!({"type": "message_stop"}));
    Reply { status: 200, content_type: "text/event-stream", body: sse(events) }
}

/// An Anthropic streaming reply whose text is `text`: the final answer
/// of an agent turn (structured output is JSON text).
/// Usage is 100 input and 50 output tokens.
pub fn sse_text(text: &str) -> Reply {
    start_and_stop(
        vec![
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": text}}),
            json!({"type": "content_block_stop", "index": 0}),
        ],
        "end_turn",
    )
}

/// An Anthropic streaming reply that calls tool `name` with `input`.
/// The call id is `toolu_1`.
/// Stops with `tool_use`.
pub fn sse_tool(name: &str, input: Value) -> Reply {
    start_and_stop(
        vec![
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "toolu_1", "name": name, "input": {}}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": input.to_string()}}),
            json!({"type": "content_block_stop", "index": 0}),
        ],
        "tool_use",
    )
}

/// A plain JSON reply with `status`.
/// Used for Anthropic non-streaming calls and Brave search.
/// Content type `application/json`.
pub fn json_reply(status: u16, body: Value) -> Reply {
    Reply { status, content_type: "application/json", body: body.to_string() }
}

/// Returns the request body as text, empty for a bodiless request.
/// Lossy for non-UTF-8 bodies.
/// Used to route fake replies by persona.
pub fn body_text(req: &HttpRequest) -> String {
    req.body.as_deref().map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default()
}

/// One scripted answer of a `ScriptTransport`: a streamed `text`, a
/// streamed `tool` call, or a plain `json` reply with its `status`.
/// Exactly one of the three is set.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptReply {
    pub text: Option<String>,
    pub tool: Option<ScriptTool>,
    pub json: Option<Value>,
    pub status: Option<u16>,
}

/// A scripted tool call: the tool name and its JSON input.
/// Streamed like a model turn that stops with `tool_use`.
/// Used for research turns.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptTool {
    pub name: String,
    pub input: Value,
}

/// One rule of a script: it matches when every `when` text is in the
/// request body and no `unless` text is. First match answers.
/// `reply` is what it answers.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRule {
    #[serde(default)]
    pub when: Vec<String>,
    #[serde(default)]
    pub unless: Vec<String>,
    pub reply: ScriptReply,
}

/// A whole fake model script: rules in order.
/// Read from the JSON file named by `AROH_FAKE_SCRIPT`.
/// A request no rule matches gets a 400.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    pub rules: Vec<ScriptRule>,
}

impl Script {
    /// Parses and checks a script: every reply sets exactly one of
    /// `text`, `tool` and `json`, and `status` only goes with `json`.
    /// Returns the script or why it is malformed.
    pub fn from_json(text: &str) -> Result<Script, String> {
        let script: Script = serde_json::from_str(text).map_err(|e| format!("fake script: {e}"))?;
        for (i, rule) in script.rules.iter().enumerate() {
            let r = &rule.reply;
            let set = [r.text.is_some(), r.tool.is_some(), r.json.is_some()].iter().filter(|s| **s).count();
            if set != 1 || (r.status.is_some() && r.json.is_none()) {
                return Err(format!("fake script rule {i}: set exactly one of text, tool, json"));
            }
        }
        Ok(script)
    }

    /// Answers `body` with the first matching rule's reply.
    /// No match answers 400 with `{"error": "no fake rule"}`.
    /// Pure, so the transport stays a thin wrapper.
    pub fn answer(&self, body: &str) -> Reply {
        let rule = self
            .rules
            .iter()
            .find(|r| r.when.iter().all(|w| body.contains(w.as_str())) && !r.unless.iter().any(|u| body.contains(u.as_str())));
        let Some(rule) = rule else {
            return json_reply(400, json!({"error": "no fake rule"}));
        };
        let r = &rule.reply;
        if let Some(text) = &r.text {
            return sse_text(text);
        }
        if let Some(tool) = &r.tool {
            return sse_tool(&tool.name, tool.input.clone());
        }
        json_reply(r.status.unwrap_or(200), r.json.clone().expect("checked by from_json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_routes_by_substrings() {
        let script = Script::from_json(
            r#"{"rules": [
                {"unless": ["\"stream\":true"], "reply": {"json": {"ok": true}}},
                {"when": ["You are Intake interviewer"], "reply": {"text": "{\"questions\": []}"}},
                {"when": ["You are Goal researcher"], "reply": {"tool": {"name": "web_search", "input": {"query": "abs"}}}}
            ]}"#,
        )
        .unwrap();
        let intake = script.answer(r#"{"stream":true,"system":"You are Intake interviewer"}"#);
        assert_eq!((intake.status, intake.content_type), (200, "text/event-stream"));
        assert!(intake.body.contains("questions"));
        let research = script.answer(r#"{"stream":true,"system":"You are Goal researcher"}"#);
        assert!(research.body.contains("web_search") && research.body.contains("tool_use"));
        let key_test = script.answer(r#"{"max_tokens":1}"#);
        assert_eq!((key_test.status, key_test.body.as_str()), (200, r#"{"ok":true}"#));
        assert_eq!(script.answer(r#"{"stream":true,"system":"You are Someone"}"#).status, 400);
        assert!(Script::from_json(r#"{"rules": [{"reply": {"text": "a", "json": {}}}]}"#).is_err());
        assert!(Script::from_json(r#"{"rules": [{"reply": {"text": "a", "status": 500}}]}"#).is_err());
        assert!(Script::from_json(r#"{"rules": [{"reply": {}, "extra": 1}]}"#).is_err());
    }
}
