use serde_json::{Value, json};

use crate::markdown::jira_rich_text;
use crate::providers::jira::JiraClient;

impl JiraClient {
    pub async fn get_worklogs(&self, issue_key: &str) -> anyhow::Result<Value> {
        self.ensure_issue_allowed(issue_key)?;
        let response = self
            .client
            .get_json(&format!("rest/api/3/issue/{issue_key}/worklog"), &[])
            .await?;
        let worklogs = response
            .get("worklogs")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|worklog| {
                        let author = worklog
                            .get("author")
                            .and_then(|author| author.get("displayName"))
                            .and_then(Value::as_str)
                            .unwrap_or("Unknown");
                        json!({
                            "id": worklog.get("id").and_then(|v| v.as_str().map(ToOwned::to_owned).or_else(|| v.as_i64().map(|n| n.to_string()))),
                            "comment": jira_rich_text(worklog.get("comment").unwrap_or(&Value::Null)),
                            "created": worklog.get("created").cloned().unwrap_or(Value::Null),
                            "updated": worklog.get("updated").cloned().unwrap_or(Value::Null),
                            "started": worklog.get("started").cloned().unwrap_or(Value::Null),
                            "time_spent": worklog.get("timeSpent").cloned().unwrap_or(Value::Null),
                            "time_spent_seconds": worklog.get("timeSpentSeconds").cloned().unwrap_or(json!(0)),
                            "author": author,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(json!({"worklogs": worklogs}))
    }
}
