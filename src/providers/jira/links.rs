use serde_json::{Value, json};

use crate::providers::jira::JiraClient;

impl JiraClient {
    pub async fn get_link_types(&self, name_filter: Option<&str>) -> anyhow::Result<Value> {
        let response = self
            .client
            .get_json("rest/api/3/issueLinkType", &[])
            .await?;
        let mut link_types: Vec<Value> = response
            .get("issueLinkTypes")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|link_type| {
                        json!({
                            "id": link_type.get("id").and_then(|v| v.as_str().map(ToOwned::to_owned).or_else(|| v.as_i64().map(|n| n.to_string()))),
                            "name": link_type.get("name").cloned().unwrap_or(Value::Null),
                            "inward": link_type.get("inward").cloned().unwrap_or(Value::Null),
                            "outward": link_type.get("outward").cloned().unwrap_or(Value::Null),
                            "self": link_type.get("self").cloned().unwrap_or(Value::Null),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(filter) = name_filter.filter(|v| !v.is_empty()) {
            let filter = filter.to_ascii_lowercase();
            link_types.retain(|link_type| {
                link_type
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.to_ascii_lowercase().contains(&filter))
            });
        }
        Ok(Value::Array(link_types))
    }
}
