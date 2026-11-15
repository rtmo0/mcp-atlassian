use serde_json::{Value, json};

use crate::providers::confluence::{ConfluenceClient, simplify_comment};

impl ConfluenceClient {
    pub async fn get_comments(&self, page_id: &str) -> anyhow::Result<Value> {
        let response = self
            .client
            .get_json(
                &format!("rest/api/content/{page_id}/child/comment"),
                &[
                    ("expand", "body.view.value,version".to_string()),
                    ("depth", "all".to_string()),
                ],
            )
            .await?;
        let comments = response
            .get("results")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(simplify_comment).collect::<Vec<_>>())
            .unwrap_or_default();
        Ok(Value::Array(comments))
    }

    #[allow(dead_code)]
    pub fn empty_comments() -> Value {
        json!([])
    }
}
