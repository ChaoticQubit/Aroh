use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who authored a message in a conversation.
/// Serializes in snake case: `system`, `user`, `assistant`, `tool`.
/// The role decides which content parts a message may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Where image or media bytes come from.
/// Either a URL the provider fetches or inline base64 data.
/// Serializes with a `kind` tag of `url` or `base64`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaSource {
    Url { url: String },
    Base64 { data: String },
}

/// One piece of a message. A message holds several, so text, images,
/// tool calls and tool results are never squeezed into one string.
/// Serializes with a `type` tag in snake case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    Image { media_type: String, source: MediaSource },
    ToolCall { id: String, name: String, arguments: Value },
    ToolResult { call_id: String, content: String, is_error: bool },
}

/// A provider-neutral chat message: a role and one or more content parts.
/// Build it with `new` to validate part placement, or `text` for plain text.
/// Providers translate it to and from their own wire format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<ContentPart>,
}

impl Message {
    /// Builds a message from a `role` and its `parts`.
    /// Returns `CoreError::InvalidMessage` for empty parts, a tool call outside
    /// the assistant role or a tool result outside the tool role.
    pub fn new(role: Role, parts: Vec<ContentPart>) -> Result<Self, CoreError> {
        if parts.is_empty() {
            return Err(CoreError::InvalidMessage("message has no parts".into()));
        }
        for part in &parts {
            match part {
                ContentPart::ToolCall { .. } if role != Role::Assistant => {
                    return Err(CoreError::InvalidMessage("tool call outside assistant message".into()))
                }
                ContentPart::ToolResult { .. } if role != Role::Tool => {
                    return Err(CoreError::InvalidMessage("tool result outside tool message".into()))
                }
                _ => {}
            }
        }
        Ok(Self { role, parts })
    }

    /// Builds a message of `role` holding one text part with `text`.
    /// Text is valid for every role, so this cannot fail.
    /// Returns the new message.
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self { role, parts: vec![ContentPart::Text { text: text.into() }] }
    }

    /// Joins the message's text parts with newlines.
    /// Images, tool calls and tool results are skipped.
    /// Returns an empty string when there is no text part.
    pub fn text_content(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Iterates over the message's tool calls in part order.
    /// Yields `(id, name, arguments)` borrowed from each call;
    /// other parts are skipped.
    pub fn tool_calls(&self) -> impl Iterator<Item = (&str, &str, &Value)> {
        self.parts.iter().filter_map(|p| match p {
            ContentPart::ToolCall { id, name, arguments } => Some((id.as_str(), name.as_str(), arguments)),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_shape_is_tagged() {
        let m = Message::text(Role::User, "hi");
        assert_eq!(serde_json::to_value(&m).unwrap(), json!({"role":"user","parts":[{"type":"text","text":"hi"}]}));
    }

    #[test]
    fn round_trips_every_part() {
        let m = Message::new(Role::Assistant, vec![
            ContentPart::Text { text: "checking".into() },
            ContentPart::ToolCall { id: "c1".into(), name: "search".into(), arguments: json!({"q":"protein"}) },
        ]).unwrap();
        let back: Message = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(back, m);
        let calls: Vec<_> = back.tool_calls().collect();
        assert_eq!(calls, vec![("c1", "search", &json!({"q":"protein"}))]);
    }

    #[test]
    fn text_content_joins_text_parts_only() {
        let m = Message::new(Role::User, vec![
            ContentPart::Text { text: "a".into() },
            ContentPart::Image { media_type: "image/png".into(), source: MediaSource::Url { url: "https://x/y.png".into() } },
            ContentPart::Text { text: "b".into() },
        ]).unwrap();
        assert_eq!(m.text_content(), "a\nb");
    }

    #[test]
    fn rejects_bad_part_placement() {
        assert!(Message::new(Role::User, vec![]).is_err());
        let call = ContentPart::ToolCall { id: "c".into(), name: "n".into(), arguments: json!({}) };
        assert!(Message::new(Role::User, vec![call]).is_err());
        let result = ContentPart::ToolResult { call_id: "c".into(), content: "ok".into(), is_error: false };
        assert!(Message::new(Role::Assistant, vec![result.clone()]).is_err());
        assert!(Message::new(Role::Tool, vec![result]).is_ok());
    }
}
