pub mod confluence;
pub mod jira;
mod pagination;
mod params;
mod render;

use std::future::Future;
use std::sync::Arc;
use std::time::Instant;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData as McpError, ServerHandler, tool_handler};

use crate::metrics::Metrics;
use crate::model::Product;
use crate::providers::Providers;
use crate::providers::confluence::ConfluenceClient;
use crate::providers::jira::JiraClient;

use self::render::{error_result, json_result};

#[derive(Clone)]
pub struct AtlassianMcpServer {
    pub(crate) providers: Providers,
    pub(crate) metrics: Arc<Metrics>,
    tool_router: ToolRouter<Self>,
}

impl AtlassianMcpServer {
    pub fn new(providers: Providers, metrics: Arc<Metrics>) -> Self {
        let tool_router = ToolRouter::new() + Self::jira_tools() + Self::confluence_tools();
        Self {
            providers,
            metrics,
            tool_router,
        }
    }

    /// Number of registered MCP tools.
    pub fn tool_count(&self) -> usize {
        self.tool_router.list_all().len()
    }

    pub(crate) async fn run_jira_json<'a, F, Fut>(
        &'a self,
        tool: &'static str,
        f: F,
    ) -> Result<CallToolResult, McpError>
    where
        F: FnOnce(&'a JiraClient) -> Fut + 'a,
        Fut: Future<Output = anyhow::Result<serde_json::Value>> + 'a,
    {
        let max = self.providers.max_content_chars;
        self.run_jira_tool(tool, |client| async move {
            f(client).await.map(|value| json_result(&value, max))
        })
        .await
    }

    pub(crate) async fn run_confluence_json<'a, F, Fut>(
        &'a self,
        tool: &'static str,
        f: F,
    ) -> Result<CallToolResult, McpError>
    where
        F: FnOnce(&'a ConfluenceClient) -> Fut + 'a,
        Fut: Future<Output = anyhow::Result<serde_json::Value>> + 'a,
    {
        let max = self.providers.max_content_chars;
        self.run_confluence_tool(tool, |client| async move {
            f(client).await.map(|value| json_result(&value, max))
        })
        .await
    }

    pub(crate) async fn run_jira_tool<'a, F, Fut>(
        &'a self,
        tool: &'static str,
        f: F,
    ) -> Result<CallToolResult, McpError>
    where
        F: FnOnce(&'a JiraClient) -> Fut,
        Fut: Future<Output = anyhow::Result<CallToolResult>> + 'a,
    {
        self.run_tool(tool, Product::Jira, self.providers.jira(), f)
            .await
    }

    pub(crate) async fn run_confluence_tool<'a, F, Fut>(
        &'a self,
        tool: &'static str,
        f: F,
    ) -> Result<CallToolResult, McpError>
    where
        F: FnOnce(&'a ConfluenceClient) -> Fut,
        Fut: Future<Output = anyhow::Result<CallToolResult>> + 'a,
    {
        self.run_tool(tool, Product::Confluence, self.providers.confluence(), f)
            .await
    }

    /// Shared tool runner: resolves the client, applies the global concurrency
    /// limit, records metrics and turns errors into MCP error results.
    async fn run_tool<'a, C, F, Fut>(
        &'a self,
        tool: &'static str,
        product: Product,
        client: anyhow::Result<&'a C>,
        f: F,
    ) -> Result<CallToolResult, McpError>
    where
        C: ?Sized,
        F: FnOnce(&'a C) -> Fut,
        Fut: Future<Output = anyhow::Result<CallToolResult>> + 'a,
    {
        let client = match client {
            Ok(client) => client,
            Err(error) => return Ok(error_result(format!("{error:#}"))),
        };
        let _permit = match self.providers.semaphore.acquire().await {
            Ok(permit) => permit,
            Err(_) => return Ok(error_result("server is shutting down")),
        };
        self.metrics.tool_started(tool);
        let started = Instant::now();
        let result = f(client).await;
        let status = if result.is_ok() { "ok" } else { "error" };
        self.metrics
            .tool_finished(tool, product.as_str(), status, started.elapsed());
        match result {
            Ok(result) => Ok(result),
            Err(error) => Ok(error_result(format!("{error:#}"))),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AtlassianMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "Atlassian MCP",
                crate::config::build_version(),
            ))
            .with_instructions(
                "Read-only Atlassian MCP server for Jira and Confluence Cloud. \
                 All tools are safe for read-only use.",
            )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::Value;
    use tokio::sync::Semaphore;

    use super::AtlassianMcpServer;
    use crate::metrics::Metrics;
    use crate::providers::Providers;

    /// Local, deliberately additive extensions to the upstream 0.21.1 schemas.
    /// Keep this list narrow: any other schema drift must fail the parity test.
    const EXTRA_PROPS: &[(&str, &[&str])] = &[
        (
            "confluence_get_page",
            &["content_offset", "content_limit", "expected_version"],
        ),
        (
            "confluence_get_page_history",
            &["content_offset", "content_limit"],
        ),
    ];

    fn test_server() -> AtlassianMcpServer {
        let providers = Providers {
            jira: None,
            confluence: None,
            semaphore: Arc::new(Semaphore::new(1)),
            max_content_chars: 1000,
            max_attachment_bytes: 1024,
            max_attachments_per_call: 1,
            max_total_attachment_bytes: 1024,
        };
        let metrics = Arc::new(Metrics::new("test", false, false).unwrap());
        AtlassianMcpServer::new(providers, metrics)
    }

    /// Upstream builds the Jira `fields` default by joining a Python `set`, so the
    /// order of the comma-separated names depends on `PYTHONHASHSEED`. Compare it
    /// as a set; every other default must match exactly.
    fn normalized_default(prop: &str, default: Option<&Value>) -> Option<Value> {
        let default = default?;
        if prop == "fields"
            && let Some(raw) = default.as_str()
        {
            let mut names: Vec<&str> = raw.split(',').map(str::trim).collect();
            names.sort_unstable();
            return Some(Value::String(names.join(",")));
        }
        Some(default.clone())
    }

    #[test]
    fn tools_match_python_contract() {
        let server = test_server();
        let tools = server.tool_router.list_all();
        assert_eq!(tools.len(), 43);

        let snapshot: Value =
            serde_json::from_str(include_str!("../../research/tools-list-0.21.1.json")).unwrap();
        let snapshot_tools = snapshot["tools"].as_array().unwrap();
        let snapshot_by_name: std::collections::HashMap<&str, &Value> = snapshot_tools
            .iter()
            .map(|tool| (tool["name"].as_str().unwrap(), tool))
            .collect();

        for tool in &tools {
            let name = tool.name.as_ref();
            let expected = snapshot_by_name
                .get(name)
                .unwrap_or_else(|| panic!("tool {name} is missing from the snapshot"));
            let expected_schema = &expected["inputSchema"];
            let actual_schema = Value::Object((*tool.input_schema).clone());
            let expected_props = expected_schema["properties"].as_object().unwrap();
            let actual_props = actual_schema["properties"].as_object().unwrap();
            let expected_keys: std::collections::BTreeSet<&str> =
                expected_props.keys().map(String::as_str).collect();
            let actual_keys: std::collections::BTreeSet<&str> =
                actual_props.keys().map(String::as_str).collect();
            let extras: std::collections::BTreeSet<&str> = EXTRA_PROPS
                .iter()
                .find(|(tool, _)| *tool == name)
                .map(|(_, props)| props.iter().copied().collect())
                .unwrap_or_default();
            let allowed: std::collections::BTreeSet<&str> =
                expected_keys.union(&extras).copied().collect();
            assert_eq!(actual_keys, allowed, "property names differ for {name}");
            let expected_required: std::collections::BTreeSet<&str> = expected_schema
                .get("required")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let actual_required: std::collections::BTreeSet<&str> = actual_schema
                .get("required")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            assert_eq!(
                actual_required, expected_required,
                "required differs for {name}"
            );
            for (prop, expected_prop) in expected_props {
                let actual_prop = &actual_props[prop];
                assert_eq!(
                    normalized_default(prop, actual_prop.get("default")),
                    normalized_default(prop, expected_prop.get("default")),
                    "default differs for {name}.{prop}"
                );
            }
        }
    }

    #[test]
    fn pagination_extensions_are_optional_without_fake_defaults() {
        let server = test_server();
        let tools = server.tool_router.list_all();
        for (name, props) in EXTRA_PROPS {
            let tool = tools
                .iter()
                .find(|tool| tool.name.as_ref() == *name)
                .unwrap();
            let schema = Value::Object((*tool.input_schema).clone());
            let required: std::collections::BTreeSet<&str> = schema
                .get("required")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            for prop in *props {
                let property = schema["properties"].get(*prop).unwrap();
                assert!(!required.contains(*prop), "{name}.{prop} must be optional");
                assert!(
                    property.get("default").is_none_or(Value::is_null),
                    "{name}.{prop} must not declare an env-dependent default"
                );
            }
        }
    }
}
