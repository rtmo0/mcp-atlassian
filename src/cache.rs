//! In-process caches for Atlassian responses.
//!
//! Two caches with different correctness models:
//!
//! - [`MetadataCache`]: short TTL for slow-changing reference data (fields, link
//!   types, projects, boards, statuses, users). Bounded staleness.
//! - [`PageCache`]: Confluence page bodies keyed by `(page_id, version, format)`.
//!   A given version never changes, so entries never go stale; the cache only
//!   bounds memory. Callers must still verify the current version before serving
//!   a cached *current* page.
//!
//! Issues, comments, search results and other live data are never cached.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use moka::future::Cache;
use moka::policy::EvictionPolicy;
use prometheus::{IntCounterVec, Opts};
use serde_json::Value;

/// Lookups per cache (`metadata` / `page`) and result (`hit` / `miss`).
pub static CACHE_LOOKUPS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new(
            "atlassian_mcp_cache_lookups_total",
            "Cache lookups by cache and result",
        ),
        &["cache", "result"],
    )
    .expect("valid cache metric definition")
});

fn record(cache: &'static str, hit: bool) {
    CACHE_LOOKUPS
        .with_label_values(&[cache, if hit { "hit" } else { "miss" }])
        .inc();
}

const METADATA_MAX_ENTRIES: u64 = 2_000;

/// TTL cache for slow-changing JSON responses, keyed by request path and query.
#[derive(Clone)]
pub struct MetadataCache {
    inner: Cache<String, Arc<Value>>,
}

impl MetadataCache {
    /// Returns `None` when `ttl` is zero (caching disabled).
    pub fn new(ttl: Duration) -> Option<Self> {
        if ttl.is_zero() {
            return None;
        }
        Some(Self {
            inner: Cache::builder()
                .max_capacity(METADATA_MAX_ENTRIES)
                .time_to_live(ttl)
                .build(),
        })
    }

    pub fn key(path: &str, query: &[(&str, String)]) -> String {
        let mut key = path.trim_start_matches('/').to_string();
        for (name, value) in query {
            key.push('\n');
            key.push_str(name);
            key.push('=');
            key.push_str(value);
        }
        key
    }

    pub async fn get(&self, key: &str) -> Option<Arc<Value>> {
        let value = self.inner.get(key).await;
        record("metadata", value.is_some());
        value
    }

    pub async fn insert(&self, key: String, value: Arc<Value>) {
        self.inner.insert(key, value).await;
    }
}

/// Which Confluence endpoint produced a cached page: the current-page response
/// carries more expansions (ancestors, history) than a historical version.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum PageKind {
    Current,
    Historical,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct PageKey {
    pub page_id: String,
    pub version: u64,
    pub markdown: bool,
    pub kind: PageKind,
}

const PAGE_IDLE: Duration = Duration::from_secs(30 * 60);

/// Size-bounded cache of simplified Confluence pages (body already converted).
#[derive(Clone)]
pub struct PageCache {
    inner: Cache<PageKey, Arc<Value>>,
}

impl PageCache {
    /// Returns `None` when `max_bytes` is zero (caching disabled).
    pub fn new(max_bytes: u64) -> Option<Self> {
        if max_bytes == 0 {
            return None;
        }
        Some(Self {
            inner: Cache::builder()
                // Plain LRU: TinyLFU admission would reject the large, rarely
                // seen pages this cache exists for.
                .eviction_policy(EvictionPolicy::lru())
                .weigher(|_key: &PageKey, value: &Arc<Value>| weight(value))
                .max_capacity(max_bytes)
                .time_to_idle(PAGE_IDLE)
                .build(),
        })
    }

    pub async fn get(&self, key: &PageKey) -> Option<Arc<Value>> {
        let value = self.inner.get(key).await;
        record("page", value.is_some());
        value
    }

    pub async fn insert(&self, key: PageKey, value: Arc<Value>) {
        self.inner.insert(key, value).await;
    }
}

/// Approximate memory footprint: the serialized size of the page.
fn weight(value: &Value) -> u32 {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
        .try_into()
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn disabled_when_zero() {
        assert!(MetadataCache::new(Duration::ZERO).is_none());
        assert!(PageCache::new(0).is_none());
    }

    #[test]
    fn metadata_key_includes_query() {
        assert_ne!(
            MetadataCache::key("rest/api/3/project", &[("startAt", "0".into())]),
            MetadataCache::key("rest/api/3/project", &[("startAt", "50".into())])
        );
        assert_eq!(
            MetadataCache::key("/rest/api/3/field", &[]),
            MetadataCache::key("rest/api/3/field", &[])
        );
    }

    #[tokio::test]
    async fn page_cache_keeps_large_page() {
        let cache = PageCache::new(10 * 1024 * 1024).unwrap();
        let key = PageKey {
            page_id: "1".into(),
            version: 3,
            markdown: true,
            kind: PageKind::Current,
        };
        let page = Arc::new(json!({"content": {"value": "x".repeat(2_000_000)}}));
        cache.insert(key.clone(), page).await;
        assert!(cache.get(&key).await.is_some());
        let other = PageKey {
            markdown: false,
            ..key
        };
        assert!(cache.get(&other).await.is_none());
    }
}
