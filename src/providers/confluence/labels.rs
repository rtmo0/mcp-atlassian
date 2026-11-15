use serde_json::Value;

use crate::providers::confluence::{ConfluenceClient, simplify_label};

impl ConfluenceClient {
    pub async fn get_labels(&self, page_id: &str) -> anyhow::Result<Value> {
        let response = self
            .client
            .get_json(
                &format!("rest/api/content/{page_id}/label"),
                &[("limit", "200".to_string())],
            )
            .await?;
        let labels = response
            .get("results")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(simplify_label).collect::<Vec<_>>())
            .unwrap_or_default();
        Ok(Value::Array(labels))
    }
}
