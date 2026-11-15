use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{ErrorData as McpError, tool, tool_router};
use serde_json::{Value, json};

use crate::server::AtlassianMcpServer;
use crate::server::pagination;
use crate::server::params::*;
use crate::server::render::{attachment_result, attachment_summary};

#[tool_router(router = confluence_tools, vis = "pub(crate)")]
impl AtlassianMcpServer {
    #[tool(
        description = "Search Confluence content using simple terms or CQL.",
        annotations(title = "Search Content", read_only_hint = true)
    )]
    async fn confluence_search(
        &self,
        Parameters(params): Parameters<ConfluenceSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_json("confluence_search", |confluence| async move {
            confluence
                .search(&params.query, params.limit, params.spaces_filter.as_deref())
                .await
        })
        .await
    }

    #[tool(
        description = "Get content of a specific Confluence page by its ID, or by its title and space key. Large pages are paginated: use content_offset/content_limit and follow content.next_call (metadata.content.next_call when include_metadata=true) until pagination.has_more is false.",
        annotations(title = "Get Page", read_only_hint = true)
    )]
    async fn confluence_get_page(
        &self,
        Parameters(params): Parameters<ConfluenceGetPageParams>,
    ) -> Result<CallToolResult, McpError> {
        let max_content_chars = self.providers.max_content_chars;
        self.run_confluence_json("confluence_get_page", |confluence| async move {
            let mut page = if let Some(page_id) = params.page_id.as_deref() {
                confluence
                    .get_page(page_id, params.convert_to_markdown)
                    .await?
            } else if let (Some(title), Some(space_key)) =
                (params.title.as_deref(), params.space_key.as_deref())
            {
                match confluence
                    .get_page_by_title(space_key, title, params.convert_to_markdown)
                    .await?
                {
                    Some(page) => page,
                    None => {
                        return Ok(json!({
                            "error": format!(
                                "Page with title '{title}' not found in space '{space_key}'."
                            )
                        }));
                    }
                }
            } else {
                return Ok(json!({
                    "error": "Either 'page_id' OR both 'title' and 'space_key' must be provided."
                }));
            };

            let offset = params.content_offset.unwrap_or(0) as usize;
            if params.content_limit == Some(0) {
                anyhow::bail!("content_limit must be greater than zero");
            }
            let limit = pagination::effective_limit(params.content_limit, max_content_chars);
            let total = pagination::content_chars(&page);
            let should_paginate = params.content_offset.is_some()
                || params.content_limit.is_some()
                || params.expected_version.is_some()
                || total > limit;

            if should_paginate {
                if offset > total {
                    anyhow::bail!("content_offset {offset} exceeds content length {total}");
                }
                if offset > 0 && params.expected_version.is_none() {
                    anyhow::bail!(
                        "expected_version is required when content_offset > 0; restart with content_offset=0"
                    );
                }
                let actual_version = pagination::page_version(&page);
                if let Some(expected) = params.expected_version {
                    match actual_version {
                        Some(actual) if actual == expected => {}
                        Some(actual) => anyhow::bail!(
                            "page changed: expected version {expected}, got {actual}; restart with content_offset=0"
                        ),
                        None => anyhow::bail!(
                            "page has no version; cannot verify expected_version; restart with content_offset=0"
                        ),
                    }
                }

                let format = if params.convert_to_markdown {
                    "markdown"
                } else {
                    "storage"
                };
                let chunk = pagination::slice_content(&mut page, offset, limit, format)?;
                let next_call = if chunk.has_more() {
                    if chunk.page_version.is_none() {
                        anyhow::bail!("page has no version; cannot paginate safely");
                    }
                    let page_id = page
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if page_id.is_empty() {
                        anyhow::bail!("page has no id; cannot build next_call");
                    }
                    Some(build_next_call(
                        "confluence_get_page",
                        json!({
                            "page_id": page_id,
                            "content_offset": chunk.next_offset(),
                            "content_limit": chunk.limit,
                            "convert_to_markdown": params.convert_to_markdown,
                            "include_metadata": params.include_metadata,
                            "expected_version": chunk.page_version,
                        }),
                    ))
                } else {
                    None
                };
                pagination::attach_pagination(&mut page, &chunk, next_call);
            }

            if params.include_metadata {
                Ok(json!({"metadata": page}))
            } else {
                let content = page
                    .get("content")
                    .cloned()
                    .unwrap_or_else(|| json!({"value": ""}));
                Ok(json!({"content": content}))
            }
        })
        .await
    }

    #[tool(
        description = "Get child pages and folders of a specific Confluence page.",
        annotations(title = "Get Page Children", read_only_hint = true)
    )]
    async fn confluence_get_page_children(
        &self,
        Parameters(params): Parameters<ConfluenceGetPageChildrenParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_json("confluence_get_page_children", |confluence| async move {
            confluence
                .get_page_children(
                    &params.parent_id,
                    &params.expand,
                    params.limit,
                    params.include_content,
                    params.convert_to_markdown,
                    params.start,
                    params.include_folders,
                )
                .await
        })
        .await
    }

    #[tool(
        description = "Get page hierarchy for a Confluence space as a flat list.",
        annotations(title = "Get Space Page Tree", read_only_hint = true)
    )]
    async fn confluence_get_space_page_tree(
        &self,
        Parameters(params): Parameters<ConfluenceGetSpacePageTreeParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_json("confluence_get_space_page_tree", |confluence| async move {
            let mut result = confluence
                .get_space_page_tree(&params.space_key, params.limit as usize)
                .await?;
            if result.get("has_more").and_then(serde_json::Value::as_bool) == Some(true) {
                result["hint"] = json!(format!(
                    "Results truncated at {} pages. Increase limit to see more.",
                    params.limit
                ));
            }
            Ok(result)
        })
        .await
    }

    #[tool(
        description = "Get comments for a specific Confluence page.",
        annotations(title = "Get Comments", read_only_hint = true)
    )]
    async fn confluence_get_comments(
        &self,
        Parameters(params): Parameters<ConfluencePageIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_json("confluence_get_comments", |confluence| async move {
            confluence.get_comments(&params.page_id).await
        })
        .await
    }

    #[tool(
        description = "Get labels for Confluence content (pages, blog posts, or attachments).",
        annotations(title = "Get Labels", read_only_hint = true)
    )]
    async fn confluence_get_labels(
        &self,
        Parameters(params): Parameters<ConfluencePageIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_json("confluence_get_labels", |confluence| async move {
            confluence.get_labels(&params.page_id).await
        })
        .await
    }

    #[tool(
        description = "Search Confluence users using CQL.",
        annotations(title = "Search User", read_only_hint = true)
    )]
    async fn confluence_search_user(
        &self,
        Parameters(params): Parameters<ConfluenceSearchUserParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_json("confluence_search_user", |confluence| async move {
            confluence.search_user(&params.query, params.limit).await
        })
        .await
    }

    #[tool(
        description = "Get a historical version of a specific Confluence page. Large versions are paginated: use content_offset/content_limit and follow content.next_call until pagination.has_more is false.",
        annotations(title = "Get Page History", read_only_hint = true)
    )]
    async fn confluence_get_page_history(
        &self,
        Parameters(params): Parameters<ConfluenceGetPageHistoryParams>,
    ) -> Result<CallToolResult, McpError> {
        let max_content_chars = self.providers.max_content_chars;
        self.run_confluence_json("confluence_get_page_history", |confluence| async move {
            let mut page = confluence
                .get_page_history(&params.page_id, params.version, params.convert_to_markdown)
                .await?;

            let offset = params.content_offset.unwrap_or(0) as usize;
            if params.content_limit == Some(0) {
                anyhow::bail!("content_limit must be greater than zero");
            }
            let limit = pagination::effective_limit(params.content_limit, max_content_chars);
            let total = pagination::content_chars(&page);
            let should_paginate =
                params.content_offset.is_some() || params.content_limit.is_some() || total > limit;

            if should_paginate {
                if offset > total {
                    anyhow::bail!("content_offset {offset} exceeds content length {total}");
                }
                let format = if params.convert_to_markdown {
                    "markdown"
                } else {
                    "storage"
                };
                let chunk = pagination::slice_content(&mut page, offset, limit, format)?;
                let next_call = if chunk.has_more() {
                    let page_id = page
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if page_id.is_empty() {
                        anyhow::bail!("page has no id; cannot build next_call");
                    }
                    Some(build_next_call(
                        "confluence_get_page_history",
                        json!({
                            "page_id": page_id,
                            "version": params.version,
                            "content_offset": chunk.next_offset(),
                            "content_limit": chunk.limit,
                            "convert_to_markdown": params.convert_to_markdown,
                        }),
                    ))
                } else {
                    None
                };
                pagination::attach_pagination(&mut page, &chunk, next_call);
            }

            Ok(page)
        })
        .await
    }

    #[tool(
        description = "Get a unified diff between two versions of a Confluence page.",
        annotations(title = "Get Page Version Diff", read_only_hint = true)
    )]
    async fn confluence_get_page_diff(
        &self,
        Parameters(params): Parameters<ConfluenceGetPageDiffParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_json("confluence_get_page_diff", |confluence| async move {
            confluence
                .get_page_diff(&params.page_id, params.from_version, params.to_version)
                .await
        })
        .await
    }

    #[tool(
        description = "Get view statistics for a Confluence page (Cloud only).",
        annotations(title = "Get Page Views", read_only_hint = true)
    )]
    async fn confluence_get_page_views(
        &self,
        Parameters(params): Parameters<ConfluenceGetPageViewsParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_json("confluence_get_page_views", |confluence| async move {
            confluence
                .get_page_views(&params.page_id, params.include_title)
                .await
        })
        .await
    }

    #[tool(
        description = "List all attachments for a Confluence content item (page or blog post).",
        annotations(title = "Get Content Attachments", read_only_hint = true)
    )]
    async fn confluence_get_attachments(
        &self,
        Parameters(params): Parameters<ConfluenceGetAttachmentsParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_json("confluence_get_attachments", |confluence| async move {
            confluence
                .get_attachments(
                    &params.content_id,
                    params.start,
                    params.limit,
                    params.filename.as_deref(),
                    params.media_type.as_deref(),
                )
                .await
        })
        .await
    }

    #[tool(
        description = "Download a Confluence attachment as an embedded MCP resource.",
        annotations(title = "Download Attachment", read_only_hint = true)
    )]
    async fn confluence_download_attachment(
        &self,
        Parameters(params): Parameters<ConfluenceAttachmentIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_tool("confluence_download_attachment", |confluence| async move {
            let batch = confluence
                .download_attachment(
                    &params.attachment_id,
                    self.providers.max_attachment_bytes,
                    self.providers.max_total_attachment_bytes,
                )
                .await?;
            let summary = attachment_summary(
                &batch,
                json!({
                    "success": true,
                    "attachment_id": params.attachment_id,
                }),
                "downloaded",
            );
            Ok(attachment_result(batch, summary, false))
        })
        .await
    }

    #[tool(
        description = "Download all attachments for a Confluence content item as embedded MCP resources.",
        annotations(title = "Download All Content Attachments", read_only_hint = true)
    )]
    async fn confluence_download_content_attachments(
        &self,
        Parameters(params): Parameters<ConfluenceContentIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_tool(
            "confluence_download_content_attachments",
            |confluence| async move {
                let batch = confluence
                    .download_content_attachments(
                        &params.content_id,
                        self.providers.max_attachment_bytes,
                        self.providers.max_attachments_per_call,
                        self.providers.max_total_attachment_bytes,
                    )
                    .await?;
                let summary = attachment_summary(
                    &batch,
                    json!({
                        "success": true,
                        "content_id": params.content_id,
                    }),
                    "downloaded",
                );
                Ok(attachment_result(batch, summary, false))
            },
        )
        .await
    }

    #[tool(
        description = "Get all images attached to a Confluence page as inline MCP image content.",
        annotations(title = "Get Page Images", read_only_hint = true)
    )]
    async fn confluence_get_page_images(
        &self,
        Parameters(params): Parameters<ConfluenceContentIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_confluence_tool("confluence_get_page_images", |confluence| async move {
            let batch = confluence
                .get_page_images(
                    &params.content_id,
                    self.providers.max_attachment_bytes,
                    self.providers.max_attachments_per_call,
                    self.providers.max_total_attachment_bytes,
                )
                .await?;
            let summary = attachment_summary(
                &batch,
                json!({
                    "success": true,
                    "content_id": params.content_id,
                }),
                "downloaded",
            );
            Ok(attachment_result(batch, summary, true))
        })
        .await
    }
}

fn build_next_call(tool: &str, arguments: Value) -> Value {
    json!({"tool": tool, "arguments": arguments})
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rmcp::handler::server::wrapper::Parameters;
    use serde_json::Value;
    use tokio::sync::Semaphore;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::config::AtlassianConfig;
    use crate::metrics::Metrics;
    use crate::providers::Providers;
    use crate::providers::confluence::ConfluenceClient;

    async fn test_server(
        content: &str,
        version: u64,
        max_content_chars: usize,
    ) -> (AtlassianMcpServer, MockServer) {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/wiki/rest/api/content/123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "123",
                "type": "page",
                "status": "current",
                "title": "Test page",
                "space": {"key": "SPC", "name": "Space"},
                "version": {
                    "number": version,
                    "when": "2026-01-01T00:00:00Z",
                    "by": {"displayName": "User"}
                },
                "history": {"createdDate": "2025-01-01T00:00:00Z"},
                "ancestors": [],
                "body": {"storage": {"value": format!("<p>{content}</p>")}},
                "children": {"attachment": {"results": []}},
                "_links": {
                    "webui": "/spaces/SPC/pages/123",
                    "base": "https://site.atlassian.net/wiki"
                }
            })))
            .mount(&mock)
            .await;

        let config = AtlassianConfig {
            base_url: mock.uri(),
            username: Some("user".to_string()),
            token: "token".to_string(),
        };
        let confluence = ConfluenceClient::new(&config, reqwest::Client::new(), 0, Vec::new());
        let providers = Providers {
            jira: None,
            confluence: Some(confluence),
            semaphore: Arc::new(Semaphore::new(1)),
            max_content_chars,
            max_attachment_bytes: 1024,
            max_attachments_per_call: 1,
            max_total_attachment_bytes: 1024,
        };
        let metrics = Arc::new(Metrics::new("test", false, true).unwrap());
        (AtlassianMcpServer::new(providers, metrics), mock)
    }

    async fn test_history_server(
        content: &str,
        version: u64,
        max_content_chars: usize,
    ) -> (AtlassianMcpServer, MockServer) {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/wiki/rest/api/content/123"))
            .and(query_param("status", "historical"))
            .and(query_param("version", version.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "123",
                "type": "page",
                "status": "historical",
                "title": "Test page",
                "space": {"key": "SPC", "name": "Space"},
                "version": {
                    "number": version,
                    "when": "2026-01-01T00:00:00Z",
                    "by": {"displayName": "User"}
                },
                "history": {"createdDate": "2025-01-01T00:00:00Z"},
                "ancestors": [],
                "body": {"storage": {"value": format!("<p>{content}</p>")}},
                "children": {"attachment": {"results": []}},
                "_links": {
                    "webui": "/spaces/SPC/pages/123",
                    "base": "https://site.atlassian.net/wiki"
                }
            })))
            .mount(&mock)
            .await;

        let config = AtlassianConfig {
            base_url: mock.uri(),
            username: Some("user".to_string()),
            token: "token".to_string(),
        };
        let confluence = ConfluenceClient::new(&config, reqwest::Client::new(), 0, Vec::new());
        let providers = Providers {
            jira: None,
            confluence: Some(confluence),
            semaphore: Arc::new(Semaphore::new(1)),
            max_content_chars,
            max_attachment_bytes: 1024,
            max_attachments_per_call: 1,
            max_total_attachment_bytes: 1024,
        };
        let metrics = Arc::new(Metrics::new("test", false, true).unwrap());
        (AtlassianMcpServer::new(providers, metrics), mock)
    }

    fn result_json(result: CallToolResult) -> Value {
        let text = result
            .structured_content
            .expect("structured content")
            .get("result")
            .and_then(Value::as_str)
            .expect("json result")
            .to_string();
        serde_json::from_str(&text).unwrap()
    }

    fn params() -> ConfluenceGetPageParams {
        ConfluenceGetPageParams {
            page_id: Some("123".to_string()),
            title: None,
            space_key: None,
            include_metadata: false,
            convert_to_markdown: true,
            content_offset: None,
            content_limit: None,
            expected_version: None,
        }
    }

    #[tokio::test]
    async fn get_page_returns_full_content_in_chunks() {
        let text = "a".repeat(250);
        let (server, _mock) = test_server(&text, 3, 100).await;

        let mut offset = 0u32;
        let mut rebuilt = String::new();
        loop {
            let mut call = params();
            call.content_offset = Some(offset);
            call.content_limit = Some(100);
            call.expected_version = Some(3);
            let result = server.confluence_get_page(Parameters(call)).await.unwrap();
            assert_ne!(result.is_error, Some(true));
            let value = result_json(result);
            let content = value["content"]["value"].as_str().unwrap();
            assert!(!content.contains("[truncated"));
            rebuilt.push_str(content);
            let pagination = &value["content"]["pagination"];
            if !pagination["has_more"].as_bool().unwrap() {
                break;
            }
            offset = pagination["next_offset"].as_u64().unwrap() as u32;
        }
        assert_eq!(rebuilt.trim_end(), text);
    }

    #[tokio::test]
    async fn get_page_first_chunk_contains_next_call() {
        let text = "a".repeat(250);
        let (server, _mock) = test_server(&text, 3, 100).await;
        let value = result_json(
            server
                .confluence_get_page(Parameters(params()))
                .await
                .unwrap(),
        );
        let pagination = &value["content"]["pagination"];
        assert_eq!(pagination["total"], 250);
        assert_eq!(pagination["returned"], 100);
        assert_eq!(pagination["next_offset"], 100);
        assert_eq!(pagination["page_version"], 3);
        assert_eq!(pagination["has_more"], true);

        let next = &value["content"]["next_call"];
        assert_eq!(next["tool"], "confluence_get_page");
        assert_eq!(next["arguments"]["page_id"], "123");
        assert_eq!(next["arguments"]["content_offset"], 100);
        assert_eq!(next["arguments"]["content_limit"], 100);
        assert_eq!(next["arguments"]["expected_version"], 3);
    }

    #[tokio::test]
    async fn get_page_requires_matching_version_for_offset() {
        let text = "a".repeat(250);
        let (server, _mock) = test_server(&text, 3, 100).await;

        let mut call = params();
        call.content_offset = Some(100);
        let result = server.confluence_get_page(Parameters(call)).await.unwrap();
        assert_eq!(result.is_error, Some(true));

        let mut call = params();
        call.content_offset = Some(100);
        call.expected_version = Some(2);
        let result = server.confluence_get_page(Parameters(call)).await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn small_page_without_params_keeps_legacy_shape() {
        let (server, _mock) = test_server("hello", 3, 100).await;
        let value = result_json(
            server
                .confluence_get_page(Parameters(params()))
                .await
                .unwrap(),
        );
        assert!(value["content"]["pagination"].is_null());
        assert!(value["content"]["next_call"].is_null());
    }

    #[tokio::test]
    async fn include_metadata_puts_pagination_under_metadata_content() {
        let text = "a".repeat(250);
        let (server, _mock) = test_server(&text, 3, 100).await;
        let mut call = params();
        call.include_metadata = true;
        call.content_limit = Some(50);
        let value = result_json(server.confluence_get_page(Parameters(call)).await.unwrap());
        assert_eq!(value["metadata"]["id"], "123");
        assert_eq!(value["metadata"]["content"]["pagination"]["limit"], 100);
        assert!(value["metadata"]["content"]["next_call"].is_object());
    }

    #[tokio::test]
    async fn page_without_content_returns_explicit_empty_chunk() {
        let (server, _mock) = test_server("", 3, 100).await;
        let mut call = params();
        call.content_offset = Some(0);
        call.content_limit = Some(50);
        let value = result_json(server.confluence_get_page(Parameters(call)).await.unwrap());
        assert_eq!(value["content"]["value"], "");
        assert_eq!(value["content"]["pagination"]["total"], 0);
        assert_eq!(value["content"]["pagination"]["has_more"], false);
    }
    #[tokio::test]
    async fn history_paginates_with_version_pinned_next_call() {
        let text = "a".repeat(250);
        let (server, _mock) = test_history_server(&text, 3, 100).await;
        let value = result_json(
            server
                .confluence_get_page_history(Parameters(ConfluenceGetPageHistoryParams {
                    page_id: "123".to_string(),
                    version: 3,
                    convert_to_markdown: true,
                    content_offset: Some(0),
                    content_limit: Some(100),
                }))
                .await
                .unwrap(),
        );
        assert_eq!(value["content"]["pagination"]["total"], 250);
        assert_eq!(value["content"]["pagination"]["page_version"], 3);
        let next = &value["content"]["next_call"];
        assert_eq!(next["tool"], "confluence_get_page_history");
        assert_eq!(next["arguments"]["version"], 3);
        assert_eq!(next["arguments"]["content_offset"], 100);
        assert_eq!(next["arguments"]["content_limit"], 100);
    }
    #[tokio::test]
    async fn get_page_rejects_zero_limit() {
        let text = "a".repeat(250);
        let (server, _mock) = test_server(&text, 3, 1000).await;
        let mut call = params();
        call.content_limit = Some(0);
        let result = server.confluence_get_page(Parameters(call)).await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn get_page_rejects_offset_beyond_total() {
        let (server, _mock) = test_server("hello", 3, 1000).await;
        let mut call = params();
        call.content_offset = Some(100);
        call.expected_version = Some(3);
        let result = server.confluence_get_page(Parameters(call)).await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn get_page_storage_mode_paginates_raw_storage() {
        let text = "a".repeat(250);
        let (server, _mock) = test_server(&text, 3, 100).await;
        let mut call = params();
        call.convert_to_markdown = false;
        call.content_offset = Some(0);
        call.content_limit = Some(100);
        let value = result_json(server.confluence_get_page(Parameters(call)).await.unwrap());
        assert_eq!(value["content"]["format"], "storage");
        assert!(
            value["content"]["value"]
                .as_str()
                .unwrap()
                .starts_with("<p>")
        );
        assert_eq!(value["content"]["pagination"]["total"], 257);
        assert_eq!(value["content"]["pagination"]["has_more"], true);
        assert_eq!(
            value["content"]["next_call"]["arguments"]["convert_to_markdown"],
            false
        );
    }

    #[tokio::test]
    async fn page_exactly_at_limit_has_no_pagination() {
        let text = "a".repeat(100);
        let (server, _mock) = test_server(&text, 3, 100).await;
        let value = result_json(
            server
                .confluence_get_page(Parameters(params()))
                .await
                .unwrap(),
        );
        assert_eq!(value["content"]["value"].as_str().unwrap().len(), 100);
        assert!(value["content"]["pagination"].is_null());
    }

    #[tokio::test]
    async fn history_small_page_keeps_legacy_shape() {
        let (server, _mock) = test_history_server("hello", 3, 100).await;
        let value = result_json(
            server
                .confluence_get_page_history(Parameters(ConfluenceGetPageHistoryParams {
                    page_id: "123".to_string(),
                    version: 3,
                    convert_to_markdown: true,
                    content_offset: None,
                    content_limit: None,
                }))
                .await
                .unwrap(),
        );
        assert!(value["content"]["pagination"].is_null());
    }
}
