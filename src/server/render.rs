use base64::Engine;
use rmcp::model::{CallToolResult, ContentBlock, ResourceContents};
use serde_json::{Value, json};

use crate::model::{AttachmentBatch, is_image_mime, json_pretty};

/// JSON text plus a structured duplicate, matching the upstream MCP server.
/// Long string values are truncated to keep MCP responses context-friendly.
pub fn json_result(value: &Value, max_chars: usize) -> CallToolResult {
    let mut value = value.clone();
    truncate_value(&mut value, max_chars);
    let text = json_pretty(&value);
    let mut result = CallToolResult::success(vec![ContentBlock::text(text.clone())]);
    result.structured_content = Some(json!({"result": text}));
    result
}

fn truncate_value(value: &mut Value, max_chars: usize) {
    match value {
        Value::String(text) if text.chars().count() > max_chars => {
            *text = crate::model::truncate_chars(text, max_chars);
        }
        Value::Array(items) => {
            for item in items {
                truncate_value(item, max_chars);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                truncate_value(item, max_chars);
            }
        }
        _ => {}
    }
}

pub fn error_result(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
}

pub fn attachment_result(
    batch: AttachmentBatch,
    summary: Value,
    images_only: bool,
) -> CallToolResult {
    let mut blocks = vec![ContentBlock::text(json_pretty(&summary))];
    for payload in batch.payloads {
        let crate::model::AttachmentPayload { attachment, bytes } = payload;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let mime = attachment.mime_type;
        let filename = attachment.filename;
        if images_only && is_image_mime(&mime) {
            blocks.push(ContentBlock::image(encoded, mime));
        } else {
            let uri = format!("attachment:///{}/{}", attachment.id, filename);
            let resource = ResourceContents::blob(encoded, uri).with_mime_type(mime);
            blocks.push(ContentBlock::resource(resource));
        }
    }
    CallToolResult::success(blocks)
}

pub fn attachment_summary(batch: &AttachmentBatch, base: Value, downloaded_key: &str) -> Value {
    let mut object = match base {
        Value::Object(object) => object,
        _ => serde_json::Map::new(),
    };
    object.insert("total".into(), json!(batch.total));
    object.insert(downloaded_key.into(), json!(batch.payloads.len()));
    object.insert("failed".into(), Value::Array(batch.failed.clone()));
    if let Some(message) = &batch.message {
        object.insert("message".into(), Value::String(message.clone()));
    }
    Value::Object(object)
}
