use serde_json::{Value, json};

use crate::model::{value_str, value_u64};
use crate::providers::jira::JiraClient;

impl JiraClient {
    pub async fn get_all_projects(&self, include_archived: bool) -> anyhow::Result<Value> {
        let mut query = Vec::new();
        if include_archived {
            query.push(("includeArchived", "true".to_string()));
        }
        let projects = self.client.get_json("rest/api/3/project", &query).await?;
        let mut projects = projects.as_array().cloned().unwrap_or_default();
        for project in &mut projects {
            if let Some(key) = project.get_mut("key")
                && let Some(text) = key.as_str()
            {
                *key = Value::String(text.to_ascii_uppercase());
            }
        }
        if !self.projects_filter.is_empty() {
            let allowed: Vec<String> = self
                .projects_filter
                .iter()
                .map(|p| p.to_ascii_uppercase())
                .collect();
            projects.retain(|project| {
                project
                    .get("key")
                    .and_then(Value::as_str)
                    .is_some_and(|key| allowed.iter().any(|allowed| allowed == key))
            });
        }
        Ok(Value::Array(projects))
    }

    pub async fn get_project_versions(&self, project_key: &str) -> anyhow::Result<Value> {
        self.ensure_project_allowed(project_key)?;
        let versions = self
            .client
            .get_json(&format!("rest/api/3/project/{project_key}/versions"), &[])
            .await?;
        let versions = versions
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|version| {
                        let mut out = serde_json::Map::new();
                        out.insert(
                            "id".into(),
                            Value::String(
                                version
                                    .get("id")
                                    .and_then(|v| {
                                        v.as_str()
                                            .map(ToOwned::to_owned)
                                            .or_else(|| v.as_i64().map(|n| n.to_string()))
                                    })
                                    .unwrap_or_default(),
                            ),
                        );
                        out.insert(
                            "name".into(),
                            Value::String(
                                value_str(version, "name").unwrap_or_default().to_string(),
                            ),
                        );
                        out.insert(
                            "released".into(),
                            Value::Bool(
                                version
                                    .get("released")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                            ),
                        );
                        out.insert(
                            "archived".into(),
                            Value::Bool(
                                version
                                    .get("archived")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                            ),
                        );
                        for field in ["description", "startDate", "releaseDate"] {
                            if let Some(value) = version.get(field).filter(|value| !value.is_null())
                            {
                                out.insert(field.into(), value.clone());
                            }
                        }
                        Value::Object(out)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(Value::Array(versions))
    }

    pub async fn get_project_components(&self, project_key: &str) -> anyhow::Result<Value> {
        self.ensure_project_allowed(project_key)?;
        self.client
            .get_json(&format!("rest/api/3/project/{project_key}/components"), &[])
            .await
    }
}

#[allow(dead_code)]
pub fn project_summary(value: &Value) -> Value {
    json!({
        "id": value.get("id").and_then(|v| v.as_str().map(ToOwned::to_owned).or_else(|| v.as_i64().map(|n| n.to_string()))),
        "key": value_str(value, "key"),
        "name": value_str(value, "name"),
        "project_type": value_str(value, "projectTypeKey"),
        "archived": value.get("archived").and_then(Value::as_bool),
        "lead": value.get("lead").and_then(|lead| lead.get("displayName")),
    })
}

#[allow(dead_code)]
pub fn total_from(value: &Value) -> u64 {
    value_u64(value, "total").unwrap_or(0)
}
