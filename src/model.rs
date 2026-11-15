use std::fmt;

use serde_json::Value;
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Product {
    Jira,
    Confluence,
}

impl Product {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jira => "jira",
            Self::Confluence => "confluence",
        }
    }
}

#[derive(Debug)]
pub struct ApiError {
    pub product: Product,
    pub status: Option<u16>,
    pub message: String,
}

impl ApiError {
    pub fn new(product: Product, status: Option<u16>, message: impl Into<String>) -> Self {
        Self {
            product,
            status,
            message: message.into(),
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status {
            Some(status) => write!(
                f,
                "{} API error (HTTP {status}): {}",
                self.product.as_str(),
                self.message
            ),
            None => write!(f, "{} API error: {}", self.product.as_str(), self.message),
        }
    }
}

impl std::error::Error for ApiError {}

#[derive(Clone, Debug)]
pub struct Attachment {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size: Option<u64>,
    pub download_url: Option<String>,
}

impl Attachment {
    pub fn from_jira(value: &Value) -> Option<Self> {
        let id = value.get("id")?.as_str()?.to_string();
        let filename = value
            .get("filename")
            .and_then(Value::as_str)
            .unwrap_or("attachment")
            .to_string();
        let mime_type = value
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or("application/octet-stream")
            .to_string();
        let size = value.get("size").and_then(Value::as_u64);
        let download_url = value
            .get("content")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        Some(Self {
            id,
            filename,
            mime_type,
            size,
            download_url,
        })
    }

    pub fn from_confluence(value: &Value, base_url: &str) -> Option<Self> {
        // v2: id/title/mediaType/fileSize/downloadLink
        // v1: id/title/extensions.mediaType/extensions.fileSize/_links.download
        let id = value.get("id")?.as_str()?.to_string();
        let filename = value
            .get("title")
            .and_then(Value::as_str)
            .or_else(|| value.get("filename").and_then(Value::as_str))
            .unwrap_or("attachment")
            .to_string();
        let mime_type = value
            .get("mediaType")
            .and_then(Value::as_str)
            .or_else(|| {
                value
                    .get("extensions")
                    .and_then(|v| v.get("mediaType"))
                    .and_then(Value::as_str)
            })
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                mime_guess::from_path(&filename)
                    .first_or_octet_stream()
                    .essence_str()
                    .to_string()
            });
        let size = value.get("fileSize").and_then(Value::as_u64).or_else(|| {
            value
                .get("extensions")
                .and_then(|v| v.get("fileSize"))
                .and_then(Value::as_u64)
        });
        let download_url = value
            .get("downloadLink")
            .and_then(Value::as_str)
            .or_else(|| {
                value
                    .get("_links")
                    .and_then(|v| v.get("download"))
                    .and_then(Value::as_str)
            })
            .map(|link| resolve_confluence_url(base_url, link));
        Some(Self {
            id,
            filename,
            mime_type,
            size,
            download_url,
        })
    }

    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "id": self.id,
            "filename": self.filename,
            "media_type": self.mime_type,
            "file_size": self.size,
            "download_url": self.download_url,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AttachmentPayload {
    pub attachment: Attachment,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct AttachmentBatch {
    pub payloads: Vec<AttachmentPayload>,
    pub failed: Vec<serde_json::Value>,
    pub total: usize,
    pub message: Option<String>,
}

pub fn is_image_mime(mime: &str) -> bool {
    mime.starts_with("image/")
}

pub fn resolve_url(base_url: &str, link: &str) -> String {
    if let Ok(absolute) = Url::parse(link)
        && (absolute.scheme() == "http" || absolute.scheme() == "https")
    {
        return absolute.to_string();
    }
    let base = base_url.trim_end_matches('/');
    if link.starts_with('/') {
        format!("{base}{link}")
    } else {
        format!("{base}/{link}")
    }
}

/// Resolve a Confluence download link against the configured base URL.
///
/// The configured URL may be either `https://site` or
/// `https://api.atlassian.com/ex/confluence/<id>` (without `/wiki`).
/// Confluence returns links such as `/wiki/download/...` and, for some
/// endpoints, `/download/...`; both must point under `/wiki`.
pub fn resolve_confluence_url(base_url: &str, link: &str) -> String {
    if let Ok(absolute) = Url::parse(link)
        && (absolute.scheme() == "http" || absolute.scheme() == "https")
    {
        return absolute.to_string();
    }
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/wiki") && link.starts_with("/wiki/") {
        format!("{base}{}", &link[5..])
    } else if link.starts_with("/wiki/") {
        format!("{base}{link}")
    } else if link.starts_with('/') {
        if base.ends_with("/wiki") {
            format!("{base}{link}")
        } else {
            format!("{base}/wiki{link}")
        }
    } else if base.ends_with("/wiki") {
        format!("{base}/{link}")
    } else {
        format!("{base}/wiki/{link}")
    }
}

pub fn json_pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

pub fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push_str(&format!(
        "\n\n… [truncated {} chars]",
        text.chars().count() - max
    ));
    out
}

pub fn value_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

pub fn value_u64(value: &Value, key: &str) -> Option<u64> {
    value
        .get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_absolute_and_relative_urls() {
        assert_eq!(
            resolve_url("https://example.com/wiki", "/download/x"),
            "https://example.com/wiki/download/x"
        );
        assert_eq!(
            resolve_url("https://example.com/wiki", "https://cdn.example.com/x"),
            "https://cdn.example.com/x"
        );
    }

    #[test]
    fn parses_confluence_v2_attachment() {
        let value = serde_json::json!({
            "id": "att1",
            "title": "picture.png",
            "mediaType": "image/png",
            "fileSize": 42,
            "downloadLink": "/wiki/download/attachments/1/picture.png"
        });
        let attachment = Attachment::from_confluence(&value, "https://example.com/wiki").unwrap();
        assert_eq!(attachment.filename, "picture.png");
        assert_eq!(attachment.mime_type, "image/png");
        assert_eq!(
            attachment.download_url.as_deref(),
            Some("https://example.com/wiki/download/attachments/1/picture.png")
        );
    }

    #[test]
    fn truncates_text() {
        assert_eq!(truncate_chars("hello", 10), "hello");
        assert!(truncate_chars("hello world", 5).contains("truncated"));
    }
}
