pub mod confluence;
pub mod jira;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use base64::Engine;
use reqwest::StatusCode;
use reqwest::header::{ACCEPT, AUTHORIZATION, RETRY_AFTER};
use serde_json::Value;
use tokio::sync::Semaphore;
use tracing::warn;

use crate::cache::{MetadataCache, PageCache};
use crate::config::{AtlassianConfig, Config};
use crate::model::{ApiError, Product};

use self::confluence::ConfluenceClient;
use self::jira::JiraClient;

/// Low-level authenticated client for one Atlassian Cloud product.
#[derive(Clone)]
pub struct AtlassianClient {
    http: reqwest::Client,
    base_url: String,
    auth_header: String,
    product: Product,
    retry_max: usize,
    api_prefix: &'static str,
    metadata_cache: Option<MetadataCache>,
}

impl AtlassianClient {
    pub fn new(
        cfg: &AtlassianConfig,
        product: Product,
        http: reqwest::Client,
        retry_max: usize,
    ) -> Self {
        let auth_header = match cfg.username.as_deref() {
            Some(username) if !username.is_empty() => {
                let raw = format!("{username}:{}", cfg.token);
                format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode(raw)
                )
            }
            _ => format!("Bearer {}", cfg.token),
        };
        Self {
            http,
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            auth_header,
            product,
            retry_max,
            api_prefix: if product == Product::Confluence {
                "/wiki"
            } else {
                ""
            },
            metadata_cache: None,
        }
    }

    pub fn with_metadata_cache(mut self, cache: Option<MetadataCache>) -> Self {
        self.metadata_cache = cache;
        self
    }

    /// GET slow-changing reference data through the metadata TTL cache.
    /// Errors are never cached. Without a cache this is a plain `get_json`.
    pub async fn get_json_cached(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> anyhow::Result<Value> {
        self.get_json_cached_inner(path, query, false).await
    }

    /// Like [`Self::get_json_cached`], but always fetches and refreshes the entry.
    pub async fn get_json_refresh(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> anyhow::Result<Value> {
        self.get_json_cached_inner(path, query, true).await
    }

    async fn get_json_cached_inner(
        &self,
        path: &str,
        query: &[(&str, String)],
        refresh: bool,
    ) -> anyhow::Result<Value> {
        let Some(cache) = &self.metadata_cache else {
            return self.get_json(path, query).await;
        };
        let key = MetadataCache::key(path, query);
        if !refresh && let Some(value) = cache.get(&key).await {
            return Ok((*value).clone());
        }
        let value = self.get_json(path, query).await?;
        cache.insert(key, Arc::new(value.clone())).await;
        Ok(value)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn api_url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            return path.to_string();
        }
        let path = path.trim_start_matches('/');
        let base = self.base_url.trim_end_matches('/');
        if self.api_prefix.is_empty() || base.ends_with(self.api_prefix) {
            format!("{base}/{path}")
        } else {
            format!("{base}{}/{path}", self.api_prefix)
        }
    }

    pub async fn get_json(&self, path: &str, query: &[(&str, String)]) -> anyhow::Result<Value> {
        self.json_request(reqwest::Method::GET, path, query, None)
            .await
    }

    /// GET a JSON document from an absolute URL, keeping the same auth.
    pub async fn get_json_absolute(&self, url: &str) -> anyhow::Result<Value> {
        let mut last_error = None;
        for attempt in 0..=self.retry_max {
            let request = self
                .http
                .get(url)
                .header(AUTHORIZATION, &self.auth_header)
                .header(ACCEPT, "application/json");
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let bytes = response
                            .bytes()
                            .await
                            .context("failed to read Atlassian response")?;
                        if bytes.is_empty() {
                            return Ok(Value::Null);
                        }
                        return serde_json::from_slice(&bytes)
                            .with_context(|| format!("non-JSON response (HTTP {status})"));
                    }
                    let retry_after = retry_after(&response);
                    let message = response_text(response).await;
                    let error = ApiError::new(self.product, Some(status.as_u16()), message);
                    if should_retry(status) && attempt < self.retry_max {
                        sleep_for(attempt, retry_after).await;
                        last_error = Some(anyhow!(error));
                        continue;
                    }
                    return Err(anyhow!(error));
                }
                Err(error) => {
                    if attempt < self.retry_max {
                        sleep_for(attempt, None).await;
                        last_error = Some(anyhow!(error));
                        continue;
                    }
                    return Err(anyhow!(
                        "{} API connection error: {error}",
                        self.product.as_str()
                    ));
                }
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow!("Atlassian request failed")))
    }

    pub async fn post_json(&self, path: &str, body: &Value) -> anyhow::Result<Value> {
        self.json_request(reqwest::Method::POST, path, &[], Some(body))
            .await
    }

    async fn json_request(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> anyhow::Result<Value> {
        let mut last_error = None;
        for attempt in 0..=self.retry_max {
            let mut request = self
                .http
                .request(method.clone(), self.api_url(path))
                .header(AUTHORIZATION, &self.auth_header)
                .header(ACCEPT, "application/json")
                .query(query);
            if let Some(body) = body {
                request = request.json(body);
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let bytes = response
                            .bytes()
                            .await
                            .context("failed to read Atlassian response")?;
                        if bytes.is_empty() {
                            return Ok(Value::Null);
                        }
                        return serde_json::from_slice(&bytes).with_context(|| {
                            format!(
                                "{} returned a non-JSON response (HTTP {status})",
                                self.product.as_str()
                            )
                        });
                    }
                    let retry_after = retry_after(&response);
                    let message = response_text(response).await;
                    let error = ApiError::new(self.product, Some(status.as_u16()), message);
                    if should_retry(status) && attempt < self.retry_max {
                        warn!(
                            product = self.product.as_str(),
                            status = status.as_u16(),
                            attempt,
                            "retrying Atlassian request"
                        );
                        sleep_for(attempt, retry_after).await;
                        last_error = Some(anyhow!(error));
                        continue;
                    }
                    return Err(anyhow!(error));
                }
                Err(error) => {
                    if attempt < self.retry_max {
                        warn!(
                            product = self.product.as_str(),
                            attempt, "Atlassian network error, retrying: {error}"
                        );
                        sleep_for(attempt, None).await;
                        last_error = Some(anyhow!(error));
                        continue;
                    }
                    return Err(anyhow!(
                        "{} API connection error: {error}",
                        self.product.as_str()
                    ));
                }
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow!("Atlassian request failed")))
    }

    /// Download bytes with a hard cap, checked before and during streaming.
    pub async fn get_bytes_limited(&self, url: &str, max_bytes: usize) -> anyhow::Result<Vec<u8>> {
        let url = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else if self.product == Product::Confluence {
            crate::model::resolve_confluence_url(&self.base_url, url)
        } else {
            crate::model::resolve_url(&self.base_url, url)
        };
        let mut last_error = None;
        for attempt in 0..=self.retry_max {
            let request = self.http.get(&url).header(AUTHORIZATION, &self.auth_header);
            match request.send().await {
                Ok(mut response) => {
                    let status = response.status();
                    if status.is_success() {
                        if let Some(length) = response.content_length()
                            && length > max_bytes as u64
                        {
                            return Err(anyhow!(ApiError::new(
                                self.product,
                                Some(413),
                                format!(
                                    "attachment is {length} bytes, exceeds the {max_bytes} byte limit"
                                ),
                            )));
                        }
                        let mut bytes = Vec::new();
                        while let Some(chunk) = response
                            .chunk()
                            .await
                            .context("failed to read attachment body")?
                        {
                            if bytes.len().saturating_add(chunk.len()) > max_bytes {
                                return Err(anyhow!(ApiError::new(
                                    self.product,
                                    Some(413),
                                    format!("attachment exceeds the {max_bytes} byte limit"),
                                )));
                            }
                            bytes.extend_from_slice(&chunk);
                        }
                        return Ok(bytes);
                    }
                    let retry_after = retry_after(&response);
                    let message = response_text(response).await;
                    let error = ApiError::new(self.product, Some(status.as_u16()), message);
                    if should_retry(status) && attempt < self.retry_max {
                        sleep_for(attempt, retry_after).await;
                        last_error = Some(anyhow!(error));
                        continue;
                    }
                    return Err(anyhow!(error));
                }
                Err(error) => {
                    if attempt < self.retry_max {
                        sleep_for(attempt, None).await;
                        last_error = Some(anyhow!(error));
                        continue;
                    }
                    return Err(anyhow!("{} download error: {error}", self.product.as_str()));
                }
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow!("Atlassian download failed")))
    }
}

fn should_retry(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .map(|value| value.min(Duration::from_secs(30)))
}

async fn sleep_for(attempt: usize, retry_after: Option<Duration>) {
    let delay = retry_after
        .unwrap_or_else(|| Duration::from_millis(300 * 2u64.saturating_pow(attempt as u32).min(8)));
    tokio::time::sleep(delay).await;
}

async fn response_text(response: reqwest::Response) -> String {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let text = text.trim();
    if text.is_empty() {
        return status
            .canonical_reason()
            .unwrap_or("empty response")
            .to_string();
    }
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(500).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[derive(Clone)]
pub struct Providers {
    pub jira: Option<JiraClient>,
    pub confluence: Option<ConfluenceClient>,
    pub semaphore: Arc<Semaphore>,
    pub max_content_chars: usize,
    pub max_attachment_bytes: usize,
    pub max_attachments_per_call: usize,
    pub max_total_attachment_bytes: usize,
}

impl Providers {
    pub fn from_config(cfg: &Config) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(cfg.request_timeout)
            .connect_timeout(Duration::from_secs(15))
            .user_agent(format!("atlassian-mcp/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .context("failed to build HTTP client")?;

        let jira = cfg.jira.as_ref().map(|jira| {
            JiraClient::new(
                jira,
                http.clone(),
                cfg.retry_max,
                cfg.jira_projects_filter.clone(),
            )
            .with_metadata_cache(MetadataCache::new(cfg.metadata_cache_ttl))
        });
        let confluence = cfg.confluence.as_ref().map(|confluence| {
            ConfluenceClient::new(
                confluence,
                http.clone(),
                cfg.retry_max,
                cfg.confluence_spaces_filter.clone(),
            )
            .with_page_cache(PageCache::new(cfg.page_cache_bytes))
        });

        Ok(Self {
            jira,
            confluence,
            semaphore: Arc::new(Semaphore::new(cfg.max_concurrency)),
            max_content_chars: cfg.max_content_chars,
            max_attachment_bytes: cfg.max_attachment_bytes,
            max_attachments_per_call: cfg.max_attachments_per_call,
            max_total_attachment_bytes: cfg.max_total_attachment_bytes,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.jira.is_none() && self.confluence.is_none()
    }

    pub fn jira(&self) -> anyhow::Result<&JiraClient> {
        self.jira
            .as_ref()
            .ok_or_else(|| anyhow!("Jira is not configured (JIRA_URL/JIRA_API_TOKEN are empty)"))
    }

    pub fn confluence(&self) -> anyhow::Result<&ConfluenceClient> {
        self.confluence.as_ref().ok_or_else(|| {
            anyhow!("Confluence is not configured (CONFLUENCE_URL/CONFLUENCE_API_TOKEN are empty)")
        })
    }
}
