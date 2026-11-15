pub mod attachments;
pub mod comments;
pub mod labels;
pub mod pages;
pub mod search;

use serde_json::{Map, Value, json};

use crate::config::AtlassianConfig;
use crate::markdown::storage_to_markdown;
use crate::model::{Attachment, Product, value_str};
use crate::providers::AtlassianClient;

#[derive(Clone)]
pub struct ConfluenceClient {
    pub(crate) client: AtlassianClient,
    pub(crate) spaces_filter: Vec<String>,
}

impl ConfluenceClient {
    pub fn new(
        cfg: &AtlassianConfig,
        http: reqwest::Client,
        retry_max: usize,
        spaces_filter: Vec<String>,
    ) -> Self {
        Self {
            client: AtlassianClient::new(cfg, Product::Confluence, http, retry_max),
            spaces_filter,
        }
    }

    pub fn base_url(&self) -> &str {
        self.client.base_url()
    }
}

pub fn simplify_page(
    value: &Value,
    base_url: &str,
    content_override: Option<&str>,
    content_format: &str,
) -> Value {
    let mut out = Map::new();
    out.insert(
        "id".into(),
        Value::String(
            value
                .get("id")
                .and_then(|v| {
                    v.as_str()
                        .map(ToOwned::to_owned)
                        .or_else(|| v.as_i64().map(|n| n.to_string()))
                })
                .unwrap_or_default(),
        ),
    );
    out.insert(
        "title".into(),
        Value::String(value_str(value, "title").unwrap_or("Untitled").to_string()),
    );
    out.insert(
        "type".into(),
        Value::String(value_str(value, "type").unwrap_or("page").to_string()),
    );
    let created = value
        .get("history")
        .and_then(|history| history.get("createdDate"))
        .and_then(Value::as_str)
        .or_else(|| value.get("createdDate").and_then(Value::as_str));
    let updated = value
        .get("version")
        .and_then(|version| version.get("when"))
        .and_then(Value::as_str);
    out.insert(
        "created".into(),
        Value::String(created.unwrap_or_default().to_string()),
    );
    out.insert(
        "updated".into(),
        Value::String(updated.unwrap_or_default().to_string()),
    );

    if let Some(space) = value.get("space") {
        out.insert(
            "space".into(),
            json!({
                "key": value_str(space, "key").unwrap_or_default(),
                "name": value_str(space, "name").unwrap_or_default(),
            }),
        );
    }
    if let Some(author) = value
        .get("version")
        .and_then(|version| version.get("by"))
        .or_else(|| {
            value
                .get("history")
                .and_then(|history| history.get("createdBy"))
        })
    {
        out.insert(
            "author".into(),
            Value::String(
                value_str(author, "displayName")
                    .or_else(|| value_str(author, "publicName"))
                    .unwrap_or_default()
                    .to_string(),
            ),
        );
    }
    if let Some(number) = value
        .get("version")
        .and_then(|version| version.get("number"))
        .and_then(Value::as_u64)
    {
        out.insert("version".into(), json!(number));
    }
    if let Some(webui) = value
        .get("_links")
        .and_then(|links| links.get("webui"))
        .and_then(Value::as_str)
    {
        let site_base = value
            .get("_links")
            .and_then(|links| links.get("base"))
            .and_then(Value::as_str)
            .unwrap_or(base_url);
        out.insert(
            "url".into(),
            Value::String(crate::model::resolve_url(site_base, webui)),
        );
    }
    if let Some(ancestors) = value.get("ancestors").and_then(Value::as_array) {
        out.insert(
            "ancestors".into(),
            Value::Array(
                ancestors
                    .iter()
                    .filter_map(|ancestor| {
                        Some(json!({
                            "id": ancestor.get("id")?.as_str()?,
                            "title": ancestor.get("title").and_then(Value::as_str).unwrap_or_default(),
                        }))
                    })
                    .collect(),
            ),
        );
    }
    if let Some(attachments) = value
        .get("children")
        .and_then(|children| children.get("attachment"))
        .and_then(|attachment| attachment.get("results"))
        .and_then(Value::as_array)
    {
        out.insert(
            "attachments".into(),
            Value::Array(
                attachments
                    .iter()
                    .map(|attachment| simplify_attachment(attachment, base_url))
                    .collect(),
            ),
        );
    } else {
        out.insert("attachments".into(), Value::Array(Vec::new()));
    }

    let content = if let Some(content) = content_override {
        Some(content.to_string())
    } else {
        value
            .get("body")
            .and_then(|body| body.get("storage"))
            .and_then(|storage| storage.get("value"))
            .and_then(Value::as_str)
            .map(|storage| {
                if content_format == "markdown" {
                    storage_to_markdown(storage)
                } else {
                    storage.to_string()
                }
            })
    };
    if let Some(content) = content
        && !content.is_empty()
    {
        out.insert(
            "content".into(),
            json!({"value": content, "format": content_format}),
        );
    }
    Value::Object(out)
}

pub fn simplify_attachment(value: &Value, base_url: &str) -> Value {
    let Some(attachment) = Attachment::from_confluence(value, base_url) else {
        return value.clone();
    };
    let version = value.get("version").unwrap_or(&Value::Null);
    let author = version
        .get("by")
        .and_then(|by| by.get("displayName"))
        .and_then(Value::as_str);
    json!({
        "id": attachment.id,
        "type": value_str(value, "type").unwrap_or("attachment"),
        "status": value_str(value, "status").unwrap_or("current"),
        "title": attachment.filename,
        "media_type": attachment.mime_type,
        "file_size": attachment.size,
        "download_url": attachment.download_url,
        "version_number": version.get("number").and_then(Value::as_u64),
        "version_when": version.get("when").and_then(Value::as_str),
        "created": value.get("created").and_then(Value::as_str),
        "author_display_name": author,
    })
}

pub fn simplify_label(value: &Value) -> Value {
    json!({
        "id": value.get("id").and_then(|v| v.as_str().map(ToOwned::to_owned).or_else(|| v.as_i64().map(|n| n.to_string()))).unwrap_or_default(),
        "name": value_str(value, "name").unwrap_or_default(),
        "prefix": value_str(value, "prefix").unwrap_or("global"),
        "label": value_str(value, "label").unwrap_or_default(),
    })
}

pub fn simplify_comment(value: &Value) -> Value {
    let body = value
        .get("body")
        .and_then(|body| body.get("view"))
        .and_then(|view| view.get("value"))
        .and_then(Value::as_str)
        .map(storage_to_markdown)
        .unwrap_or_default();
    let author = value
        .get("author")
        .and_then(|author| author.get("displayName"))
        .or_else(|| {
            value
                .get("version")
                .and_then(|version| version.get("by"))
                .and_then(|by| by.get("displayName"))
        })
        .and_then(Value::as_str);
    let parent_comment_id = value
        .get("container")
        .filter(|container| container.get("type").and_then(Value::as_str) == Some("comment"))
        .and_then(|container| container.get("id"))
        .and_then(Value::as_str);
    let mut out = Map::new();
    out.insert(
        "id".into(),
        Value::String(
            value
                .get("id")
                .and_then(|v| {
                    v.as_str()
                        .map(ToOwned::to_owned)
                        .or_else(|| v.as_i64().map(|n| n.to_string()))
                })
                .unwrap_or_default(),
        ),
    );
    out.insert("body".into(), Value::String(body));
    out.insert(
        "created".into(),
        value.get("created").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "updated".into(),
        value.get("updated").cloned().unwrap_or(Value::Null),
    );
    if let Some(title) = value_str(value, "title") {
        out.insert("title".into(), Value::String(title.to_string()));
    }
    if let Some(author) = author {
        out.insert("author".into(), Value::String(author.to_string()));
    }
    if let Some(parent) = parent_comment_id {
        out.insert(
            "parent_comment_id".into(),
            Value::String(parent.to_string()),
        );
    }
    if let Some(location) = value
        .get("extensions")
        .and_then(|extensions| extensions.get("location"))
        .and_then(Value::as_str)
    {
        out.insert("location".into(), Value::String(location.to_string()));
    }
    Value::Object(out)
}
