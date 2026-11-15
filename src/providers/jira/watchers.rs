use serde_json::{Value, json};

use crate::providers::jira::{JiraClient, simplify_user};

impl JiraClient {
    pub async fn get_issue_watchers(&self, issue_key: &str) -> anyhow::Result<Value> {
        self.ensure_issue_allowed(issue_key)?;
        let response = self
            .client
            .get_json(&format!("rest/api/3/issue/{issue_key}/watchers"), &[])
            .await?;
        let watchers = response
            .get("watchers")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(simplify_user).collect::<Vec<_>>())
            .unwrap_or_default();
        Ok(json!({
            "issue_key": issue_key,
            "watcher_count": response.get("watchCount").cloned().unwrap_or(json!(watchers.len())),
            "is_watching": response.get("isWatching").cloned().unwrap_or(json!(false)),
            "watchers": watchers,
        }))
    }
}
