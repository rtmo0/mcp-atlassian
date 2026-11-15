mod adf;
mod storage;

pub use adf::adf_to_markdown;
pub use storage::storage_to_markdown;

/// Convert a Jira rich-text value (ADF object, plain string or null) to Markdown.
pub fn jira_rich_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Object(_) => adf_to_markdown(value),
        other => other.to_string(),
    }
}
