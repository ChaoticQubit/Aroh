mod claude;
pub mod fetch;
pub mod model;
pub mod steps;
pub mod template;

pub use claude::{Claude, DEFAULT_MODEL};
pub use fetch::{
    CourseText, MAX_TEXT_CHARS, fetch_course, html_to_text, is_public_ip, pasted_text,
};
pub use steps::{
    CourseAnalysis, CourseModule, PlannedAction, Question, analyze_course, generate_plan,
    validate_plan, write_questions,
};
pub use template::{AgentDecl, Effort, Rendered, builtin, parse_agent, render};

/// Every failure this crate reports. The payload says what went wrong in words.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("no Claude API key")]
    MissingKey,
    #[error("http: {0}")]
    Http(String),
    #[error("model: {0}")]
    Model(String),
    #[error("invalid json: {0}")]
    InvalidJson(String),
    #[error("fetch: {0}")]
    Fetch(String),
    #[error("template: {0}")]
    Template(String),
    #[error("validation: {0}")]
    Validation(String),
}
