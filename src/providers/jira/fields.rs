use serde_json::{Value, json};

use crate::model::value_str;
use crate::providers::jira::JiraClient;

impl JiraClient {
    pub async fn get_fields(&self) -> anyhow::Result<Value> {
        self.client.get_json("rest/api/3/field", &[]).await
    }

    pub async fn search_fields(&self, keyword: &str, limit: usize) -> anyhow::Result<Value> {
        let fields = self.get_fields().await?;
        let fields = fields.as_array().cloned().unwrap_or_default();
        if keyword.trim().is_empty() {
            return Ok(Value::Array(
                fields.into_iter().take(limit).collect::<Vec<_>>(),
            ));
        }
        let needle = keyword.trim().to_ascii_lowercase();
        let mut matched: Vec<(u8, Value)> = fields
            .into_iter()
            .filter_map(|field| {
                let name = value_str(&field, "name")
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let key = value_str(&field, "key")
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let id = value_str(&field, "id")
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let score = if name == needle || key == needle || id == needle {
                    0
                } else if name.starts_with(&needle) || key.starts_with(&needle) {
                    1
                } else if name.contains(&needle) || key.contains(&needle) {
                    2
                } else {
                    return None;
                };
                Some((score, field))
            })
            .collect();
        matched.sort_by_key(|(score, _)| *score);
        Ok(Value::Array(
            matched
                .into_iter()
                .take(limit)
                .map(|(_, field)| field)
                .collect(),
        ))
    }

    pub async fn get_field_options(
        &self,
        field_id: &str,
        context_id: Option<&str>,
        contains: Option<&str>,
        return_limit: Option<usize>,
        values_only: bool,
    ) -> anyhow::Result<Value> {
        let context_id = match context_id.filter(|v| !v.is_empty()) {
            Some(context_id) => context_id.to_string(),
            None => self.resolve_field_context(field_id).await?,
        };
        let mut options = Vec::new();
        let mut start_at = 0u64;
        loop {
            let response = self
                .client
                .get_json(
                    &format!("rest/api/3/field/{field_id}/context/{context_id}/option"),
                    &[
                        ("startAt", start_at.to_string()),
                        ("maxResults", "1000".to_string()),
                    ],
                )
                .await?;
            let values = response
                .get("values")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if values.is_empty() {
                break;
            }
            let count = values.len() as u64;
            options.extend(values);
            start_at += count;
            if response
                .get("isLast")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            {
                break;
            }
            if start_at > 20_000 {
                break;
            }
        }
        let mut options: Vec<Value> = options.iter().map(simplify_option).collect();
        if let Some(needle) = contains.filter(|v| !v.is_empty()) {
            let needle = needle.to_ascii_lowercase();
            options.retain(|option| option_matches(option, &needle));
        }
        if let Some(limit) = return_limit {
            options.truncate(limit);
        }
        if values_only {
            let values: Vec<Value> = options
                .iter()
                .map(|option| {
                    let children = option
                        .get("child_options")
                        .and_then(Value::as_array)
                        .map(|children| {
                            children
                                .iter()
                                .filter_map(|child| child.get("value").cloned())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if children.is_empty() {
                        option.get("value").cloned().unwrap_or(Value::Null)
                    } else {
                        json!({
                            "value": option.get("value").cloned().unwrap_or(Value::Null),
                            "children": children,
                        })
                    }
                })
                .collect();
            return Ok(Value::Array(values));
        }
        Ok(Value::Array(options))
    }

    async fn resolve_field_context(&self, field_id: &str) -> anyhow::Result<String> {
        let response = self
            .client
            .get_json(&format!("rest/api/3/field/{field_id}/context"), &[])
            .await?;
        let contexts = response
            .get("values")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let global = contexts.iter().find(|context| {
            context
                .get("isGlobalContext")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
        let chosen = global.or_else(|| contexts.first());
        chosen
            .and_then(|context| {
                context.get("id").and_then(|v| {
                    v.as_str()
                        .map(ToOwned::to_owned)
                        .or_else(|| v.as_i64().map(|n| n.to_string()))
                })
            })
            .ok_or_else(|| anyhow::anyhow!("field {field_id} has no contexts"))
    }
}

fn simplify_option(value: &Value) -> Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "id".into(),
        Value::String(
            value
                .get("id")
                .or_else(|| value.get("optionId"))
                .and_then(|v| {
                    v.as_str()
                        .map(ToOwned::to_owned)
                        .or_else(|| v.as_i64().map(|n| n.to_string()))
                })
                .unwrap_or_default(),
        ),
    );
    out.insert(
        "value".into(),
        Value::String(
            value_str(value, "value")
                .or_else(|| value_str(value, "name"))
                .unwrap_or_default()
                .to_string(),
        ),
    );
    if value
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        out.insert("disabled".into(), Value::Bool(true));
    }
    if let Some(children) = value.get("cascadingOptions").and_then(Value::as_array)
        && !children.is_empty()
    {
        out.insert(
            "child_options".into(),
            Value::Array(children.iter().map(simplify_option).collect()),
        );
    }
    Value::Object(out)
}

fn option_matches(option: &Value, needle: &str) -> bool {
    if option
        .get("value")
        .and_then(Value::as_str)
        .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
    {
        return true;
    }
    option
        .get("child_options")
        .and_then(Value::as_array)
        .is_some_and(|children| children.iter().any(|child| option_matches(child, needle)))
}
