use serde_json::{Map, Value, json};

use crate::providers::jira::JiraClient;

impl JiraClient {
    pub async fn get_issue_development_info(
        &self,
        issue_key: &str,
        application_type: Option<&str>,
        data_type: Option<&str>,
    ) -> anyhow::Result<Value> {
        self.ensure_issue_allowed(issue_key)?;
        let issue = self
            .client
            .get_json(
                &format!("rest/api/3/issue/{issue_key}"),
                &[("fields", "id".to_string())],
            )
            .await?;
        let issue_id = issue
            .get("id")
            .and_then(|v| {
                v.as_str()
                    .map(ToOwned::to_owned)
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .ok_or_else(|| anyhow::anyhow!("could not resolve the ID of issue {issue_key}"))?;

        if let Some(application_type) = application_type.filter(|v| !v.is_empty()) {
            return self
                .fetch_dev_info(
                    issue_key,
                    &issue_id,
                    application_type,
                    data_type.filter(|v| !v.is_empty()),
                )
                .await;
        }

        let mut merged = empty_development(issue_key);
        for application_type in ["stash", "bitbucket", "github", "gitlab"] {
            for data_type in ["pullrequest", "branch", "repository"] {
                match self
                    .fetch_dev_info(issue_key, &issue_id, application_type, Some(data_type))
                    .await
                {
                    Ok(result) => merge_development(&mut merged, &result),
                    Err(error) => {
                        if merged.get("error").is_none() {
                            merged["error"] = Value::String(format!("{error:#}"));
                        }
                    }
                }
            }
            if merged.get("error").is_some() {
                break;
            }
        }
        Ok(merged)
    }

    async fn fetch_dev_info(
        &self,
        issue_key: &str,
        issue_id: &str,
        application_type: &str,
        data_type: Option<&str>,
    ) -> anyhow::Result<Value> {
        let mut query = vec![
            ("issueId", issue_id.to_string()),
            ("applicationType", application_type.to_string()),
        ];
        if let Some(data_type) = data_type {
            query.push(("dataType", data_type.to_string()));
        }
        match self
            .client
            .get_json("rest/dev-status/1.0/issue/detail", &query)
            .await
        {
            Ok(response) => Ok(parse_development(issue_key, &response)),
            Err(error) => {
                if let Some(api_error) = error.downcast_ref::<crate::model::ApiError>() {
                    if api_error.status == Some(404) {
                        return Ok(json!({
                            "issue_key": issue_key,
                            "error": "Development info is not available — the Jira dev-status plugin may not be installed on this instance.",
                            "detail": [],
                            "pullRequests": [],
                            "branches": [],
                            "commits": [],
                            "repositories": [],
                        }));
                    }
                    if api_error.status == Some(403) {
                        return Ok(json!({
                            "issue_key": issue_key,
                            "error": "Access denied to development info — check Jira permissions or dev-status plugin configuration.",
                            "detail": [],
                            "pullRequests": [],
                            "branches": [],
                            "commits": [],
                            "repositories": [],
                        }));
                    }
                }
                Err(error)
            }
        }
    }

    pub async fn get_issues_development_info(
        &self,
        issue_keys: &[String],
        application_type: Option<&str>,
        data_type: Option<&str>,
    ) -> anyhow::Result<Value> {
        let mut results = Vec::new();
        for issue_key in issue_keys {
            let result = self
                .get_issue_development_info(issue_key, application_type, data_type)
                .await
                .unwrap_or_else(|error| {
                    json!({
                        "issue_key": issue_key,
                        "error": format!("{error:#}"),
                    })
                });
            results.push(result);
        }
        Ok(Value::Array(results))
    }
}

fn empty_development(issue_key: &str) -> Value {
    json!({
        "issue_key": issue_key,
        "detail": [],
        "pullRequests": [],
        "branches": [],
        "commits": [],
        "repositories": [],
    })
}

fn parse_development(issue_key: &str, response: &Value) -> Value {
    let mut out = empty_development(issue_key);
    if let Some(detail) = response.get("detail").and_then(Value::as_array) {
        out["detail"] = Value::Array(detail.clone());
        for entry in detail {
            merge_development(&mut out, entry);
        }
    }
    out
}

fn merge_development(target: &mut Value, source: &Value) {
    let mut additions: Vec<(String, Vec<Value>)> = Vec::new();
    for key in ["pullRequests", "branches", "commits", "repositories"] {
        if let Some(values) = source.get(key).and_then(Value::as_array) {
            additions.push((key.to_string(), values.clone()));
        }
    }
    if let Some(detail) = source.get("detail").and_then(Value::as_array) {
        for entry in detail {
            merge_development(target, entry);
        }
    }
    let Some(target_object) = target.as_object_mut() else {
        return;
    };
    for (key, values) in additions {
        let target_values = target_object
            .entry(key)
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(target_array) = target_values.as_array_mut() {
            for value in values {
                if !target_array.contains(&value) {
                    target_array.push(value);
                }
            }
        }
    }
}

#[allow(dead_code)]
fn object_or_empty(value: Option<&Value>) -> Map<String, Value> {
    value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}
