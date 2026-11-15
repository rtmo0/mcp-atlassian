use std::time::Duration;

use anyhow::{Context, bail};
use tracing::warn;
use url::Url;

/// Connection settings for one Atlassian product.
#[derive(Clone)]
pub struct AtlassianConfig {
    /// Base URL without a trailing slash. For Confluence this includes `/wiki`.
    pub base_url: String,
    /// Cloud uses Basic auth (email + API token). Empty means Bearer auth.
    pub username: Option<String>,
    pub token: String,
}

impl AtlassianConfig {
    fn from_env(prefix: &str, is_confluence: bool) -> anyhow::Result<Option<Self>> {
        let url_name = format!("{prefix}_URL");
        let user_name = format!("{prefix}_USERNAME");
        let token_name = format!("{prefix}_API_TOKEN");

        let raw_url = match non_empty(&url_name) {
            Some(url) => url,
            None => {
                if non_empty(&token_name).is_some() {
                    bail!("{token_name} is set but {url_name} is empty");
                }
                return Ok(None);
            }
        };
        let token = match non_empty(&token_name) {
            Some(token) => token,
            None => bail!("{url_name} is set but {token_name} is empty"),
        };

        let base_url = normalize_base_url(&raw_url, is_confluence)?;

        Ok(Some(Self {
            base_url,
            username: non_empty(&user_name),
            token,
        }))
    }
}

/// Settings shared by every transport: Atlassian access, limits and filters.
#[derive(Clone)]
pub struct Config {
    pub jira: Option<AtlassianConfig>,
    pub confluence: Option<AtlassianConfig>,
    pub request_timeout: Duration,
    pub retry_max: usize,
    pub max_concurrency: usize,
    pub max_content_chars: usize,
    pub max_attachment_bytes: usize,
    pub max_attachments_per_call: usize,
    pub max_total_attachment_bytes: usize,
    pub jira_projects_filter: Vec<String>,
    pub confluence_spaces_filter: Vec<String>,
    pub read_only: bool,
    /// TTL of the Jira metadata cache; zero disables it.
    pub metadata_cache_ttl: Duration,
    /// Memory budget of the Confluence page cache in bytes; zero disables it.
    pub page_cache_bytes: u64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let read_only = env_bool("READ_ONLY_MODE", true);
        if !read_only {
            warn!(
                "READ_ONLY_MODE=false is ignored: write tools are not implemented, the server stays read-only"
            );
        }

        let jira = AtlassianConfig::from_env("JIRA", false)?;
        let confluence = AtlassianConfig::from_env("CONFLUENCE", true)?;
        if jira.is_none() && confluence.is_none() {
            warn!("neither Jira nor Confluence is configured: every tool will return an error");
        }

        let request_timeout =
            Duration::from_secs(env_parse::<u64>("ATLASSIAN_MCP_REQUEST_TIMEOUT_SECS", 60)?)
                .max(Duration::from_secs(1));
        let retry_max = env_parse("ATLASSIAN_MCP_RETRY_MAX", 2usize)?;
        let max_concurrency = env_parse("ATLASSIAN_MCP_MAX_CONCURRENCY", 4usize)?.max(1);
        let max_content_chars =
            env_parse("ATLASSIAN_MCP_MAX_CONTENT_CHARS", 100_000usize)?.max(1000);
        let max_attachment_bytes =
            env_parse("ATLASSIAN_MCP_MAX_ATTACHMENT_BYTES", 50 * 1024 * 1024usize)?.max(1024);
        let max_attachments_per_call =
            env_parse("ATLASSIAN_MCP_MAX_ATTACHMENTS_PER_CALL", 20usize)?.max(1);
        let max_total_attachment_bytes = env_parse(
            "ATLASSIAN_MCP_MAX_TOTAL_ATTACHMENT_BYTES",
            100 * 1024 * 1024usize,
        )?
        .max(max_attachment_bytes);
        let metadata_cache_ttl =
            Duration::from_secs(env_parse("ATLASSIAN_MCP_METADATA_CACHE_TTL_SECS", 300u64)?);
        let page_cache_bytes =
            env_parse("ATLASSIAN_MCP_PAGE_CACHE_MB", 64u64)?.saturating_mul(1024 * 1024);

        Ok(Self {
            jira,
            confluence,
            request_timeout,
            retry_max,
            max_concurrency,
            max_content_chars,
            max_attachment_bytes,
            max_attachments_per_call,
            max_total_attachment_bytes,
            jira_projects_filter: parse_csv("JIRA_PROJECTS_FILTER"),
            confluence_spaces_filter: parse_csv("CONFLUENCE_SPACES_FILTER"),
            read_only,
            metadata_cache_ttl,
            page_cache_bytes,
        })
    }
}

/// Settings that only apply to the Streamable HTTP transport (`serve`).
#[derive(Clone)]
pub struct HttpConfig {
    pub port: u16,
    pub listen: String,
    pub api_token: Option<String>,
    pub require_auth: bool,
    pub allowed_hosts: Vec<String>,
    pub metrics_port: u16,
    pub metrics_listen: String,
}

impl HttpConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let api_token = non_empty("ATLASSIAN_MCP_API_TOKEN");
        let require_auth = env_bool("ATLASSIAN_MCP_REQUIRE_AUTH", false);
        if require_auth && api_token.is_none() {
            bail!("ATLASSIAN_MCP_REQUIRE_AUTH is enabled but ATLASSIAN_MCP_API_TOKEN is empty");
        }
        if api_token.is_none() {
            warn!(
                "ATLASSIAN_MCP_API_TOKEN is not set: /mcp is unauthenticated (local development only)"
            );
        }

        let port = env_parse("ATLASSIAN_MCP_PORT", 8080u16)?;
        let listen =
            std::env::var("ATLASSIAN_MCP_LISTEN").unwrap_or_else(|_| "127.0.0.1".to_string());
        let allowed_hosts = parse_allowed_hosts(
            std::env::var("ATLASSIAN_MCP_ALLOWED_HOSTS")
                .unwrap_or_else(|_| DEFAULT_ALLOWED_HOSTS.to_string())
                .as_str(),
        )?;
        let metrics_port = env_parse("ATLASSIAN_MCP_METRICS_PORT", 9090u16)?;
        let metrics_listen = std::env::var("ATLASSIAN_MCP_METRICS_LISTEN")
            .unwrap_or_else(|_| "127.0.0.1".to_string());

        Ok(Self {
            port,
            listen,
            api_token,
            require_auth,
            allowed_hosts,
            metrics_port,
            metrics_listen,
        })
    }
}

const DEFAULT_ALLOWED_HOSTS: &str = "localhost,127.0.0.1,::1";

fn normalize_base_url(raw: &str, is_confluence: bool) -> anyhow::Result<String> {
    let parsed = Url::parse(raw).with_context(|| format!("invalid URL: {raw}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        bail!("URL must use http or https: {raw}");
    }
    if parsed.host_str().is_none() {
        bail!("URL must contain a host: {raw}");
    }
    let mut out = raw.trim().trim_end_matches('/').to_string();
    if is_confluence {
        // Accept both https://site/wiki and https://site; normalize later.
        if out.ends_with("/wiki") {
            out.truncate(out.len() - 5);
        }
    }
    Ok(out)
}

fn parse_allowed_hosts(raw: &str) -> anyhow::Result<Vec<String>> {
    let hosts: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if hosts.is_empty() {
        bail!(
            "ATLASSIAN_MCP_ALLOWED_HOSTS is empty: list at least one Host \
             (an empty list would disable rmcp DNS-rebinding protection)"
        );
    }
    Ok(hosts)
}

fn parse_csv(name: &str) -> Vec<String> {
    std::env::var(name)
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub fn build_version() -> String {
    std::env::var("ATLASSIAN_MCP_BUILD_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn env_parse<T>(name: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(raw) if !raw.trim().is_empty() => raw
            .trim()
            .parse()
            .map_err(|error| anyhow::anyhow!("{name}: {error}")),
        _ => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_allowed_hosts_and_trims() {
        let hosts = parse_allowed_hosts(" example.com, localhost ,,").unwrap();
        assert_eq!(hosts, vec!["example.com", "localhost"]);
    }

    #[test]
    fn rejects_empty_allowed_hosts() {
        assert!(parse_allowed_hosts("").is_err());
        assert!(parse_allowed_hosts(" , ").is_err());
    }

    #[test]
    fn normalizes_urls() {
        assert_eq!(
            normalize_base_url("https://api.atlassian.com/ex/jira/abc/", false).unwrap(),
            "https://api.atlassian.com/ex/jira/abc"
        );
        assert_eq!(
            normalize_base_url("https://api.atlassian.com/ex/confluence/abc/wiki/", true).unwrap(),
            "https://api.atlassian.com/ex/confluence/abc"
        );
    }

    #[test]
    fn rejects_non_http_urls() {
        assert!(normalize_base_url("ftp://example.com", false).is_err());
        assert!(normalize_base_url("not a url", false).is_err());
    }
}
