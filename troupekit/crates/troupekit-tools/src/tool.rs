use async_trait::async_trait;
use serde_json::Value;
use troupekit_core::ids::RunId;
use troupekit_core::tool::ToolSpec;

/// Per-call context handed to every tool.
/// Cheap to clone, so typed tool closures can own a copy across awaits.
/// `run_id` names the agent run that made the call, when there is one.
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    pub run_id: Option<RunId>,
}

/// What a tool returns to the model.
/// `is_error` marks content that describes a failure the model may react to,
/// such as an HTTP error status from a remote API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    /// Builds a successful output holding `content`.
    /// Returns the output with `is_error` false.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    /// Builds an error output holding `content`.
    /// Returns the output with `is_error` true.
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }

    /// Folds a tool result into what the model sees.
    /// `Ok` passes through; `Err` becomes an error output whose content is
    /// the error's display text, which never holds secret values.
    pub fn from_result(result: Result<ToolOutput, ToolError>) -> Self {
        result.unwrap_or_else(|e| Self::error(e.to_string()))
    }
}

/// Why a tool call or tool setup failed.
/// Unknown tools, schema-invalid arguments, blocked addresses, transport
/// and decode failures and bad configuration each have their own variant.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToolError {
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("duplicate tool: {0}")]
    DuplicateTool(String),
    #[error("invalid tool spec: {0}")]
    InvalidSpec(String),
    #[error("config: {0}")]
    Config(String),
    #[error("blocked: {0}")]
    Blocked(String),
    #[error("http: {0}")]
    Http(String),
    #[error("decode: {0}")]
    Decode(String),
    #[error("failed: {0}")]
    Failed(String),
}

/// A capability an agent can call: a spec the model sees and an async call.
/// Object safe, so registries hold `Arc<dyn Tool>`. Arguments arrive as JSON
/// already checked against `spec().schema` when called through a registry.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Returns the name, description and argument schema the model sees.
    fn spec(&self) -> &ToolSpec;

    /// Runs the tool with JSON `args` and the per-call `ctx`.
    /// Returns the model-facing output or a `ToolError`.
    async fn call(&self, args: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;
}
