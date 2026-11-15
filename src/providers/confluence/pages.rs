use serde_json::{Value, json};
use similar::TextDiff;

use crate::markdown::storage_to_markdown;
use crate::model::{value_str, value_u64};
use crate::providers::confluence::{ConfluenceClient, simplify_page};

const PAGE_EXPAND: &str = "body.storage,version,space,history,ancestors,children.attachment";

impl ConfluenceClient {
    pub async fn get_page(
        &self,
        page_id: &str,
        convert_to_markdown: bool,
    ) -> anyhow::Result<Value> {
        let page = self
            .client
            .get_json(
                &format!("rest/api/content/{page_id}"),
                &[("expand", PAGE_EXPAND.to_string())],
            )
            .await?;
        Ok(simplify_page(
            &page,
            self.base_url(),
            None,
            if convert_to_markdown {
                "markdown"
            } else {
                "storage"
            },
        ))
    }

    pub async fn get_page_by_title(
        &self,
        space_key: &str,
        title: &str,
        convert_to_markdown: bool,
    ) -> anyhow::Result<Option<Value>> {
        let response = self
            .client
            .get_json(
                "rest/api/content",
                &[
                    ("title", title.to_string()),
                    ("spaceKey", space_key.to_string()),
                    ("expand", PAGE_EXPAND.to_string()),
                ],
            )
            .await?;
        let page = response
            .get("results")
            .and_then(Value::as_array)
            .and_then(|results| results.first());
        Ok(page.map(|page| {
            simplify_page(
                page,
                self.base_url(),
                None,
                if convert_to_markdown {
                    "markdown"
                } else {
                    "storage"
                },
            )
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn get_page_children(
        &self,
        parent_id: &str,
        expand: &str,
        limit: u32,
        include_content: bool,
        convert_to_markdown: bool,
        start: u32,
        include_folders: bool,
    ) -> anyhow::Result<Value> {
        let limit = limit.clamp(1, 50);
        let mut expand = expand.to_string();
        if include_content && !expand.contains("body") {
            if expand.is_empty() {
                expand = "body.storage".to_string();
            } else {
                expand.push_str(",body.storage");
            }
        }
        let mut items = self
            .child_items(parent_id, "page", start, limit, &expand)
            .await?;
        if include_folders
            && let Ok(mut folders) = self
                .child_items(parent_id, "folder", start, limit, &expand)
                .await
        {
            items.append(&mut folders);
        }
        let results = items
            .iter()
            .map(|item| {
                let content = if include_content {
                    item.get("body")
                        .and_then(|body| body.get("storage"))
                        .and_then(|storage| storage.get("value"))
                        .and_then(Value::as_str)
                        .map(|storage| {
                            if convert_to_markdown {
                                storage_to_markdown(storage)
                            } else {
                                storage.to_string()
                            }
                        })
                } else {
                    None
                };
                simplify_page(
                    item,
                    self.base_url(),
                    content.as_deref(),
                    if convert_to_markdown {
                        "markdown"
                    } else {
                        "storage"
                    },
                )
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "parent_id": parent_id,
            "count": results.len(),
            "limit_requested": limit,
            "start_requested": start,
            "results": results,
        }))
    }

    async fn child_items(
        &self,
        parent_id: &str,
        child_type: &str,
        start: u32,
        limit: u32,
        expand: &str,
    ) -> anyhow::Result<Vec<Value>> {
        let response = self
            .client
            .get_json(
                &format!("rest/api/content/{parent_id}/child/{child_type}"),
                &[
                    ("start", start.to_string()),
                    ("limit", limit.to_string()),
                    ("expand", expand.to_string()),
                ],
            )
            .await?;
        Ok(response
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    pub async fn get_space_page_tree(
        &self,
        space_key: &str,
        limit: usize,
    ) -> anyhow::Result<Value> {
        let limit = limit.min(1000);
        let page_size = 200u32;
        let mut start = 0u32;
        let mut all_pages: Vec<Value> = Vec::new();
        let mut has_next = false;
        while all_pages.len() < limit {
            let fetch_limit = (page_size as usize).min(limit - all_pages.len()) as u32;
            let response = self
                .client
                .get_json(
                    "rest/api/content",
                    &[
                        ("spaceKey", space_key.to_string()),
                        ("type", "page".to_string()),
                        ("start", start.to_string()),
                        ("limit", fetch_limit.to_string()),
                        ("expand", "ancestors".to_string()),
                    ],
                )
                .await?;
            let batch = response
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if batch.is_empty() {
                break;
            }
            all_pages.extend(batch);
            has_next = response
                .get("_links")
                .and_then(|links| links.get("next"))
                .is_some();
            if !has_next {
                break;
            }
            start += fetch_limit;
        }
        let mut pages: Vec<Value> = all_pages
            .iter()
            .map(|page| {
                let ancestors = page
                    .get("ancestors")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let parent_id = ancestors
                    .last()
                    .and_then(|ancestor| ancestor.get("id"))
                    .and_then(Value::as_str);
                json!({
                    "id": page.get("id").and_then(|v| v.as_str().map(ToOwned::to_owned).or_else(|| v.as_i64().map(|n| n.to_string()))),
                    "title": value_str(page, "title").unwrap_or("Untitled"),
                    "parent_id": parent_id,
                    "position": page.get("extensions").and_then(|extensions| extensions.get("position")),
                    "depth": ancestors.len(),
                })
            })
            .collect();
        pages.sort_by(|a, b| {
            let depth_a = a.get("depth").and_then(Value::as_u64).unwrap_or(0);
            let depth_b = b.get("depth").and_then(Value::as_u64).unwrap_or(0);
            depth_a.cmp(&depth_b).then_with(|| {
                let pos_a = a
                    .get("position")
                    .and_then(Value::as_u64)
                    .unwrap_or(u64::MAX);
                let pos_b = b
                    .get("position")
                    .and_then(Value::as_u64)
                    .unwrap_or(u64::MAX);
                pos_a.cmp(&pos_b)
            })
        });
        let mut result = json!({
            "space_key": space_key,
            "total_pages": pages.len(),
            "has_more": has_next && all_pages.len() >= limit,
            "pages": pages,
        });
        if result["has_more"].as_bool().unwrap_or(false) {
            result["next_start"] = json!(start);
        }
        Ok(result)
    }

    pub async fn get_page_history(
        &self,
        page_id: &str,
        version: u32,
        convert_to_markdown: bool,
    ) -> anyhow::Result<Value> {
        let page = self
            .client
            .get_json(
                &format!("rest/api/content/{page_id}"),
                &[
                    ("status", "historical".to_string()),
                    ("version", version.to_string()),
                    (
                        "expand",
                        "body.storage,version,space,children.attachment".to_string(),
                    ),
                ],
            )
            .await?;
        Ok(simplify_page(
            &page,
            self.base_url(),
            None,
            if convert_to_markdown {
                "markdown"
            } else {
                "storage"
            },
        ))
    }

    pub async fn get_page_diff(
        &self,
        page_id: &str,
        from_version: u32,
        to_version: u32,
    ) -> anyhow::Result<Value> {
        let from = self.get_page_history(page_id, from_version, true).await?;
        let to = self.get_page_history(page_id, to_version, true).await?;
        let from_content = from
            .get("content")
            .and_then(|content| content.get("value"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let to_content = to
            .get("content")
            .and_then(|content| content.get("value"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let diff = TextDiff::from_lines(from_content, to_content)
            .unified_diff()
            .header(&format!("v{from_version}"), &format!("v{to_version}"))
            .to_string();
        Ok(json!({
            "page_id": page_id,
            "title": to.get("title").cloned().unwrap_or(Value::Null),
            "from_version": from_version,
            "to_version": to_version,
            "diff": diff,
        }))
    }

    pub async fn get_page_views(
        &self,
        page_id: &str,
        include_title: bool,
    ) -> anyhow::Result<Value> {
        let title = if include_title {
            self.client
                .get_json(
                    &format!("rest/api/content/{page_id}"),
                    &[("expand", "title".to_string())],
                )
                .await
                .ok()
                .and_then(|page| value_str(&page, "title").map(ToOwned::to_owned))
        } else {
            None
        };
        let response = self
            .client
            .get_json(&format!("rest/api/analytics/content/{page_id}/views"), &[])
            .await?;
        let total_views = value_u64(&response, "count").unwrap_or(0);
        let last_viewed = value_str(&response, "lastSeen");
        let mut out = json!({
            "page_id": page_id,
            "total_views": total_views,
        });
        if let Some(title) = title {
            out["page_title"] = Value::String(title);
        }
        if let Some(last_viewed) = last_viewed {
            out["last_viewed"] = Value::String(last_viewed.to_string());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::config::AtlassianConfig;
    use crate::providers::confluence::ConfluenceClient;

    #[tokio::test]
    async fn get_page_uses_v1_api_and_converts_storage() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/wiki/rest/api/content/123"))
            .and(header("authorization", "Basic dXNlcjp0b2tlbg=="))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "123",
                "type": "page",
                "status": "current",
                "title": "Test page",
                "space": {"key": "SPC", "name": "Space"},
                "version": {
                    "number": 3,
                    "when": "2026-01-01T00:00:00Z",
                    "by": {"displayName": "User"}
                },
                "history": {"createdDate": "2025-01-01T00:00:00Z"},
                "ancestors": [],
                "body": {"storage": {"value": "<p>Hello <strong>world</strong></p>"}},
                "children": {"attachment": {"results": []}},
                "_links": {
                    "webui": "/spaces/SPC/pages/123",
                    "base": "https://site.atlassian.net/wiki"
                }
            })))
            .mount(&server)
            .await;

        let config = AtlassianConfig {
            base_url: server.uri(),
            username: Some("user".to_string()),
            token: "token".to_string(),
        };
        let client = ConfluenceClient::new(&config, reqwest::Client::new(), 0, Vec::new());
        let page = client.get_page("123", true).await.unwrap();
        assert_eq!(page["title"], "Test page");
        assert_eq!(
            page["url"],
            "https://site.atlassian.net/wiki/spaces/SPC/pages/123"
        );
        assert!(
            page["content"]["value"]
                .as_str()
                .unwrap()
                .contains("Hello **world**")
        );
        assert_eq!(page["space"]["key"], "SPC");
    }
}
