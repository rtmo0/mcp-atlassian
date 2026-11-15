use serde_json::{Value, json};

use crate::providers::jira::JiraClient;

impl JiraClient {
    pub async fn get_transitions(&self, issue_key: &str) -> anyhow::Result<Value> {
        self.ensure_issue_allowed(issue_key)?;
        let response = self
            .client
            .get_json(
                &format!("rest/api/3/issue/{issue_key}/transitions"),
                &[("expand", "transitions.fields".to_string())],
            )
            .await?;
        let transitions = response
            .get("transitions")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|transition| {
                        let to_status = transition
                            .get("to")
                            .and_then(|to| to.get("name"))
                            .and_then(Value::as_str);
                        json!({
                            "id": transition.get("id").cloned().unwrap_or(Value::Null),
                            "name": transition.get("name").cloned().unwrap_or(Value::Null),
                            "to_status": to_status,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(Value::Array(transitions))
    }
}
