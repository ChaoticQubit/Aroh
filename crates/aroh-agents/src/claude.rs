use rig_core::ErrorKind;
use rig_core::completion::{CompletionRequest, Effort as RigEffort, Reasoning};
use rig_core::providers::anthropic::Anthropic;

use crate::AgentError;
use crate::template::Effort;

/// Model every Aroh agent uses unless [`Claude::with_model`] picks another.
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";

const MAX_TOKENS: u64 = 16_000;

/// Claude client over rig's Anthropic provider. Sends no `temperature`,
/// no prefill and no `tool_choice`, which Opus 5.5 rejects; structured
/// output goes through `output_config.format`, effort through
/// `output_config.effort`.
#[derive(Clone)]
pub struct Claude {
    provider: Anthropic,
    model: String,
}

impl std::fmt::Debug for Claude {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Claude")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl Claude {
    /// Client for `api_key`. A blank key is [`AgentError::MissingKey`].
    pub fn new(api_key: &str) -> Result<Claude, AgentError> {
        let key = api_key.trim();
        if key.is_empty() {
            return Err(AgentError::MissingKey);
        }
        Ok(Claude {
            provider: Anthropic::new(key.to_owned()),
            model: DEFAULT_MODEL.to_owned(),
        })
    }

    /// The same client addressing `model`.
    pub fn with_model(self, model: &str) -> Claude {
        assert!(!model.trim().is_empty(), "model id must not be blank");
        Claude {
            model: model.to_owned(),
            ..self
        }
    }

    /// One turn at `medium` effort. Returns the reply's JSON text, checked
    /// to parse. With `schema`, Claude is held to it by structured output.
    pub async fn complete_json(
        &self,
        system: &str,
        user: &str,
        schema: Option<&serde_json::Value>,
    ) -> Result<String, AgentError> {
        self.complete(system, user, schema, Effort::Medium).await
    }

    /// [`Claude::complete_json`] at the given effort.
    pub(crate) async fn complete(
        &self,
        system: &str,
        user: &str,
        schema: Option<&serde_json::Value>,
        effort: Effort,
    ) -> Result<String, AgentError> {
        let effort = match effort {
            Effort::Low => RigEffort::Low,
            Effort::Medium => RigEffort::Medium,
            Effort::High => RigEffort::High,
        };
        let mut request = CompletionRequest::new(user.to_owned())
            .preamble(system.to_owned())
            .max_tokens(MAX_TOKENS)
            .reasoning(Reasoning::Effort(effort));
        if let Some(schema) = schema {
            let schema = rig_core::schemars::Schema::try_from(schema.clone())
                .map_err(|e| AgentError::InvalidJson(format!("output schema: {e}")))?;
            request = request.output_schema(schema);
        }
        assert!(request.temperature.is_none() && request.tool_choice.is_none());
        let response = self
            .provider
            .completion(self.model.clone())
            .call(request)
            .await
            .map_err(|e| match e.kind() {
                ErrorKind::Http | ErrorKind::Timeout | ErrorKind::Url | ErrorKind::Cancelled => {
                    AgentError::Http(e.to_string())
                }
                _ => AgentError::Model(e.to_string()),
            })?;
        let text = strip_fences(&response.text()).to_owned();
        serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|e| AgentError::InvalidJson(format!("{e}: {}", preview(&text))))?;
        Ok(text)
    }
}

/// `text` without surrounding whitespace or one Markdown code fence.
fn strip_fences(text: &str) -> &str {
    let text = text.trim();
    let Some(body) = text.strip_prefix("```") else {
        return text;
    };
    let body = body.strip_prefix("json").unwrap_or(body);
    body.strip_suffix("```").unwrap_or(body).trim()
}

/// The first 200 characters of `text`, for error messages.
fn preview(text: &str) -> String {
    text.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig_core::providers::anthropic::AnthropicConfig;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn capture_one(
        reply: &'static str,
    ) -> (String, tokio::task::JoinHandle<serde_json::Value>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            let body = loop {
                let n = sock.read(&mut chunk).await.unwrap();
                assert!(n > 0, "client closed before sending a body");
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).into_owned();
                let Some((head, body)) = text.split_once("\r\n\r\n") else {
                    continue;
                };
                let length: usize = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap();
                if body.len() >= length {
                    break body[..length].to_owned();
                }
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
            sock.write_all(response.as_bytes()).await.unwrap();
            serde_json::from_str(&body).unwrap()
        });
        (base, task)
    }

    const REPLY: &str = r#"{"id":"msg_1","type":"message","role":"assistant","model":"claude-opus-5-5","content":[{"type":"text","text":"```json\n{\"ok\": true}\n```"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":3,"output_tokens":4}}"#;

    #[tokio::test]
    async fn request_follows_opus_rules() {
        let (base, task) = capture_one(REPLY).await;
        let claude = Claude {
            provider: AnthropicConfig::new("test-key")
                .with_base_url(&base)
                .client(),
            model: DEFAULT_MODEL.into(),
        };
        let schema = serde_json::json!({"type": "object", "properties": {"ok": {"type": "boolean"}}, "required": ["ok"], "additionalProperties": false});
        let text = claude
            .complete("sys", "hi", Some(&schema), Effort::High)
            .await
            .unwrap();
        assert_eq!(text, "{\"ok\": true}");
        let body = task.await.unwrap();
        for banned in ["temperature", "top_p", "top_k", "tool_choice"] {
            assert!(body.get(banned).is_none(), "{banned} sent: {body}");
        }
        assert_eq!(body["model"], DEFAULT_MODEL);
        assert_eq!(body["output_config"]["effort"], "high");
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(
            messages.last().unwrap()["role"],
            "user",
            "no assistant prefill: {body}"
        );
    }

    #[test]
    fn blank_key_is_missing() {
        assert!(matches!(Claude::new("  "), Err(AgentError::MissingKey)));
    }
}
