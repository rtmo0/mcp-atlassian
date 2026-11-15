use serde_json::{Value, json};

use crate::model::value_str;
use crate::providers::jira::JiraClient;

impl JiraClient {
    pub async fn get_agile_boards(
        &self,
        board_name: Option<&str>,
        project_key: Option<&str>,
        board_type: Option<&str>,
        start_at: u32,
        limit: u32,
    ) -> anyhow::Result<Value> {
        let mut query = vec![
            ("startAt", start_at.to_string()),
            ("maxResults", limit.clamp(1, 50).to_string()),
        ];
        if let Some(name) = board_name.filter(|v| !v.is_empty()) {
            query.push(("name", name.to_string()));
        }
        if let Some(project) = project_key.filter(|v| !v.is_empty()) {
            query.push(("projectKeyOrId", project.to_string()));
        }
        if let Some(kind) = board_type.filter(|v| !v.is_empty()) {
            query.push(("type", kind.to_string()));
        }
        let response = self.client.get_json("rest/agile/1.0/board", &query).await?;
        let boards = response
            .get("values")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(simplify_board).collect::<Vec<_>>())
            .unwrap_or_default();
        Ok(Value::Array(boards))
    }

    pub async fn get_sprints_from_board(
        &self,
        board_id: &str,
        state: Option<&str>,
        start_at: u32,
        limit: u32,
    ) -> anyhow::Result<Value> {
        let mut query = vec![
            ("startAt", start_at.to_string()),
            ("maxResults", limit.clamp(1, 50).to_string()),
        ];
        if let Some(state) = state.filter(|v| !v.is_empty()) {
            query.push(("state", state.to_string()));
        }
        let response = self
            .client
            .get_json(&format!("rest/agile/1.0/board/{board_id}/sprint"), &query)
            .await?;
        let sprints = response
            .get("values")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(simplify_sprint).collect::<Vec<_>>())
            .unwrap_or_default();
        Ok(Value::Array(sprints))
    }
}

pub fn simplify_board(value: &Value) -> Value {
    json!({
        "id": value.get("id").and_then(|v| v.as_str().map(ToOwned::to_owned).or_else(|| v.as_i64().map(|n| n.to_string()))),
        "name": value_str(value, "name"),
        "type": value_str(value, "type"),
    })
}

pub fn simplify_sprint(value: &Value) -> Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "id".into(),
        Value::String(
            value
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
        Value::String(value_str(value, "name").unwrap_or_default().to_string()),
    );
    out.insert(
        "state".into(),
        Value::String(value_str(value, "state").unwrap_or_default().to_string()),
    );
    if let Some(goal) = value_str(value, "goal").filter(|v| !v.is_empty()) {
        out.insert("goal".into(), Value::String(goal.to_string()));
    }
    for field in ["startDate", "endDate"] {
        if let Some(value) = value_str(value, field).filter(|v| !v.is_empty()) {
            let key = if field == "startDate" {
                "start_date"
            } else {
                "end_date"
            };
            out.insert(key.into(), Value::String(value.to_string()));
        }
    }
    Value::Object(out)
}
