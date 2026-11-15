use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value, json};

/// Default chunk for large Confluence page bodies. Deliberately independent of
/// `ATLASSIAN_MCP_MAX_CONTENT_CHARS`: raising the global cap should not turn a
/// single tool response into an unbounded context bomb.
pub const DEFAULT_CHUNK_CHARS: usize = 100_000;

/// Lower bound for `content_limit`; avoids pathological 1-char pagination.
pub const MIN_CHUNK_CHARS: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentPagination {
    pub offset: usize,
    pub limit: usize,
    pub returned: usize,
    pub total: usize,
    pub page_version: Option<u64>,
}

impl ContentPagination {
    pub fn has_more(&self) -> bool {
        self.offset + self.returned < self.total
    }

    pub fn next_offset(&self) -> Option<usize> {
        self.has_more().then_some(self.offset + self.returned)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "offset": self.offset,
            "limit": self.limit,
            "returned": self.returned,
            "total": self.total,
            "has_more": self.has_more(),
            "next_offset": self.next_offset(),
            "page_version": self.page_version,
        })
    }
}

pub fn effective_limit(requested: Option<u32>, max_content_chars: usize) -> usize {
    let max = max_content_chars.max(1);
    let min = MIN_CHUNK_CHARS.min(max);
    requested
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_CHUNK_CHARS.min(max))
        .clamp(min, max)
}

pub fn content_chars(page: &Value) -> usize {
    page.pointer("/content/value")
        .and_then(Value::as_str)
        .map(|value| value.chars().count())
        .unwrap_or(0)
}

pub fn page_version(page: &Value) -> Option<u64> {
    page.get("version").and_then(Value::as_u64)
}

/// Slice `page.content.value` to `[offset, offset + limit)` in Unicode scalar
/// values. The content object is created when missing so active pagination
/// always exposes `value` and `format`.
pub fn slice_content(
    page: &mut Value,
    offset: usize,
    limit: usize,
    format: &str,
) -> Result<ContentPagination> {
    if limit == 0 {
        bail!("content_limit must be greater than zero");
    }
    let total = content_chars(page);
    if offset > total {
        bail!("content_offset {offset} exceeds content length {total}");
    }
    let value = page
        .pointer("/content/value")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let chunk: String = value.chars().skip(offset).take(limit).collect();
    let returned = chunk.chars().count();
    let page_version = page_version(page);

    let object = page
        .as_object_mut()
        .ok_or_else(|| anyhow!("page is not a JSON object"))?;
    let content = object
        .entry("content")
        .or_insert_with(|| Value::Object(Map::new()));
    let content = content
        .as_object_mut()
        .ok_or_else(|| anyhow!("page content is not a JSON object"))?;
    content.insert("value".into(), Value::String(chunk));
    content
        .entry("format")
        .or_insert_with(|| Value::String(format.to_string()));

    Ok(ContentPagination {
        offset,
        limit,
        returned,
        total,
        page_version,
    })
}

/// Attach pagination metadata to `page.content`. `next_call` is included only
/// when supplied.
pub fn attach_pagination(
    page: &mut Value,
    pagination: &ContentPagination,
    next_call: Option<Value>,
) {
    let Some(content) = page.get_mut("content").and_then(Value::as_object_mut) else {
        return;
    };
    content.insert("pagination".into(), pagination.to_json());
    if let Some(next_call) = next_call {
        content.insert("next_call".into(), next_call);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_page(text: &str) -> Value {
        json!({
            "id": "6264782851",
            "version": 89,
            "content": {"format": "markdown", "value": text}
        })
    }

    fn chunk(text: &str, offset: usize, limit: usize) -> (String, ContentPagination) {
        let mut page = sample_page(text);
        let pagination = slice_content(&mut page, offset, limit, "markdown").unwrap();
        let value = page
            .pointer("/content/value")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        (value, pagination)
    }

    #[test]
    fn slices_ascii_in_a_chain() {
        let (value, first) = chunk("abcdefghij", 0, 4);
        assert_eq!(first.returned, 4);
        assert_eq!(first.total, 10);
        assert_eq!(first.next_offset(), Some(4));
        assert_eq!(value, "abcd");

        let (value, second) = chunk("abcdefghij", 4, 4);
        assert_eq!(value, "efgh");
        assert_eq!(second.next_offset(), Some(8));

        let (value, third) = chunk("abcdefghij", 8, 4);
        assert_eq!(value, "ij");
        assert_eq!(third.next_offset(), None);
        assert!(!third.has_more());
    }

    #[test]
    fn slices_unicode_by_chars() {
        let text = "Привет 👋🏽 мир";
        let mut rebuilt = String::new();
        let mut offset = 0;
        loop {
            let (value, pagination) = chunk(text, offset, 3);
            rebuilt.push_str(&value);
            match pagination.next_offset() {
                Some(next) => offset = next,
                None => break,
            }
        }
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn offset_at_total_returns_empty_chunk() {
        let mut page = sample_page("abc");
        let result = slice_content(&mut page, 3, 10, "markdown").unwrap();
        assert_eq!(page.pointer("/content/value").unwrap(), "");
        assert_eq!(result.returned, 0);
        assert!(!result.has_more());
        assert_eq!(result.next_offset(), None);
    }

    #[test]
    fn offset_beyond_total_is_rejected() {
        let mut page = sample_page("abc");
        let error = slice_content(&mut page, 4, 10, "markdown").unwrap_err();
        assert!(error.to_string().contains("exceeds content length"));
    }

    #[test]
    fn missing_content_is_created_for_active_pagination() {
        let mut page = json!({"id": "1", "version": 2});
        let result = slice_content(&mut page, 0, 10, "storage").unwrap();
        assert_eq!(page.pointer("/content/value").unwrap(), "");
        assert_eq!(page.pointer("/content/format").unwrap(), "storage");
        assert_eq!(result.total, 0);
        assert!(!result.has_more());
    }

    #[test]
    fn zero_limit_is_rejected() {
        let mut page = sample_page("abc");
        assert!(slice_content(&mut page, 0, 0, "markdown").is_err());
    }

    #[test]
    fn pagination_json_exposes_next_offset_only_when_more() {
        let mut page = sample_page("abcdef");
        let first = slice_content(&mut page, 0, 4, "markdown").unwrap();
        let json = first.to_json();
        assert_eq!(json["has_more"], true);
        assert_eq!(json["next_offset"], 4);
        assert_eq!(json["page_version"], 89);

        let last = slice_content(&mut page, 4, 4, "markdown").unwrap();
        let json = last.to_json();
        assert_eq!(json["has_more"], false);
        assert!(json["next_offset"].is_null());
    }

    #[test]
    fn attach_pagination_adds_next_call_when_supplied() {
        let mut page = sample_page("abcdef");
        let first = slice_content(&mut page, 0, 4, "markdown").unwrap();
        attach_pagination(
            &mut page,
            &first,
            Some(json!({"tool": "x", "arguments": {}})),
        );
        assert!(page.pointer("/content/pagination").is_some());
        assert!(page.pointer("/content/next_call").is_some());

        let mut page = sample_page("abcdef");
        let last = slice_content(&mut page, 4, 4, "markdown").unwrap();
        attach_pagination(&mut page, &last, None);
        assert!(page.pointer("/content/pagination").is_some());
        assert!(page.pointer("/content/next_call").is_none());
    }

    #[test]
    fn effective_limit_uses_safe_default_and_clamps() {
        assert_eq!(effective_limit(None, 100_000), 100_000);
        assert_eq!(effective_limit(None, 500_000), 100_000);
        assert_eq!(effective_limit(Some(20_000), 100_000), 20_000);
        assert_eq!(effective_limit(Some(1), 100_000), MIN_CHUNK_CHARS);
        assert_eq!(effective_limit(Some(2_000_000), 100_000), 100_000);
        assert_eq!(effective_limit(None, 500), 500);
    }
}
