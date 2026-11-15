use serde_json::{Value, json};

use crate::markdown::storage_to_markdown;
use crate::model::{value_str, value_u64};
use crate::providers::confluence::{ConfluenceClient, simplify_page};

impl ConfluenceClient {
    pub async fn search(
        &self,
        query: &str,
        limit: u32,
        spaces_filter: Option<&str>,
    ) -> anyhow::Result<Value> {
        let cql = if is_simple_query(query) {
            format!("siteSearch ~ \"{}\"", escape_cql(query))
        } else {
            query.to_string()
        };
        let filter = spaces_filter
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.spaces_filter.join(","));
        let cql = apply_spaces_filter(&cql, &filter);

        let response = match self.search_cql(&cql, limit).await {
            Ok(response) => response,
            Err(error) if is_simple_query(query) => {
                let fallback =
                    apply_spaces_filter(&format!("text ~ \"{}\"", escape_cql(query)), &filter);
                match self.search_cql(&fallback, limit).await {
                    Ok(response) => response,
                    Err(_) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };

        let results = response
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let pages = results
            .iter()
            .map(|result| {
                let content = result.get("content").unwrap_or(result);
                let excerpt = result.get("excerpt").and_then(Value::as_str);
                if let Some(excerpt) = excerpt {
                    tracing::debug!(excerpt_len = excerpt.len(), "confluence search excerpt");
                }
                let excerpt = excerpt.map(storage_to_markdown);
                if let Some(excerpt) = &excerpt {
                    tracing::debug!(
                        converted_len = excerpt.len(),
                        "confluence search converted excerpt"
                    );
                }
                let mut page =
                    simplify_page(content, self.base_url(), excerpt.as_deref(), "markdown");
                if page.get("space").is_none()
                    && let Some(container) = result.get("resultGlobalContainer")
                {
                    let display_url = value_str(container, "displayUrl").unwrap_or_default();
                    let key = display_url
                        .strip_prefix("/spaces/")
                        .unwrap_or(display_url)
                        .trim_end_matches('/');
                    page["space"] = json!({
                        "key": key,
                        "name": value_str(container, "title").unwrap_or_default(),
                    });
                }
                page
            })
            .collect::<Vec<_>>();
        Ok(Value::Array(pages))
    }

    async fn search_cql(&self, cql: &str, limit: u32) -> anyhow::Result<Value> {
        self.client
            .get_json(
                "rest/api/search",
                &[
                    ("cql", cql.to_string()),
                    ("limit", limit.clamp(1, 50).to_string()),
                ],
            )
            .await
    }

    pub async fn search_user(&self, query: &str, limit: u32) -> anyhow::Result<Value> {
        let cql = if is_simple_user_query(query) {
            format!("user.fullname ~ \"{}\"", escape_cql(query))
        } else {
            query.to_string()
        };
        let response = self
            .client
            .get_json(
                "rest/api/search/user",
                &[("cql", cql), ("limit", limit.clamp(1, 50).to_string())],
            )
            .await?;
        let results = response
            .get("results")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|result| {
                        let user = result.get("user").unwrap_or(&Value::Null);
                        let mut out = serde_json::Map::new();
                        out.insert(
                            "entity_type".into(),
                            Value::String(
                                value_str(result, "entityType")
                                    .unwrap_or("user")
                                    .to_string(),
                            ),
                        );
                        if let Some(title) = value_str(result, "title") {
                            out.insert("title".into(), Value::String(title.to_string()));
                        }
                        out.insert(
                            "score".into(),
                            result.get("score").cloned().unwrap_or(json!(0.0)),
                        );
                        if !user.is_null() {
                            let mut user_out = serde_json::Map::new();
                            user_out.insert(
                                "account_id".into(),
                                user.get("accountId").cloned().unwrap_or(Value::Null),
                            );
                            user_out.insert(
                                "display_name".into(),
                                Value::String(
                                    value_str(user, "displayName")
                                        .unwrap_or_default()
                                        .to_string(),
                                ),
                            );
                            user_out.insert(
                                "email".into(),
                                user.get("email").cloned().unwrap_or(Value::Null),
                            );
                            user_out.insert(
                                "profile_picture".into(),
                                user.get("profilePicture")
                                    .and_then(|picture| picture.get("path"))
                                    .cloned()
                                    .unwrap_or(Value::Null),
                            );
                            user_out.insert(
                                "is_active".into(),
                                user.get("accountStatus")
                                    .and_then(Value::as_str)
                                    .map(|status| Value::Bool(status == "active"))
                                    .unwrap_or(Value::Bool(true)),
                            );
                            out.insert("user".into(), Value::Object(user_out));
                        }
                        if let Some(url) = value_str(result, "url") {
                            out.insert("url".into(), Value::String(url.to_string()));
                        }
                        if let Some(last_modified) = value_str(result, "lastModified") {
                            out.insert(
                                "last_modified".into(),
                                Value::String(last_modified.to_string()),
                            );
                        }
                        if let Some(excerpt) = value_str(result, "excerpt") {
                            out.insert("excerpt".into(), Value::String(excerpt.to_string()));
                        }
                        Value::Object(out)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(Value::Array(results))
    }
}

fn is_simple_query(query: &str) -> bool {
    !query.is_empty()
        && !["=", "~", ">", "<", " AND ", " OR ", "currentUser()"]
            .iter()
            .any(|token| query.contains(token))
}

fn is_simple_user_query(query: &str) -> bool {
    !query.is_empty()
        && !["=", "~", ">", "<", " AND ", " OR ", "user."]
            .iter()
            .any(|token| query.contains(token))
}

fn apply_spaces_filter(cql: &str, filter: &str) -> String {
    let spaces: Vec<&str> = filter
        .split(',')
        .map(str::trim)
        .filter(|space| !space.is_empty())
        .collect();
    if spaces.is_empty() || cql.to_ascii_lowercase().contains("space = ") {
        return cql.to_string();
    }
    let space_query = spaces
        .iter()
        .map(|space| format!("space = \"{}\"", escape_cql(space)))
        .collect::<Vec<_>>()
        .join(" OR ");
    if cql.trim().is_empty() {
        space_query
    } else {
        format!("({cql}) AND ({space_query})")
    }
}

fn escape_cql(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[allow(dead_code)]
fn total_size(value: &Value) -> u64 {
    value_u64(value, "totalSize").unwrap_or(0)
}
