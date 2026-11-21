use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A tool as a model sees it: its name, what it does and a JSON Schema
/// for its arguments. Providers send it with a completion request and
/// tools crates build it from their tool definitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trips_as_plain_fields() {
        let t = ToolSpec { name: "search".into(), description: "Web search".into(), schema: json!({"type":"object"}) };
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v, json!({"name":"search","description":"Web search","schema":{"type":"object"}}));
        assert_eq!(serde_json::from_value::<ToolSpec>(v).unwrap(), t);
    }
}
