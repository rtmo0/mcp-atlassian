use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::model::value_str;
use crate::providers::jira::JiraClient;

#[derive(Clone, Debug)]
pub struct StatusChange {
    pub status: String,
    pub entered_at: DateTime<Utc>,
    pub exited_at: Option<DateTime<Utc>>,
    pub transitioned_by: Option<String>,
}

impl StatusChange {
    pub fn duration_minutes(&self, now: DateTime<Utc>) -> i64 {
        let end = self.exited_at.unwrap_or(now);
        (end - self.entered_at).num_minutes().max(0)
    }
}

#[derive(Clone, Debug)]
pub struct StatusSummary {
    pub status: String,
    pub total_minutes: i64,
    pub visit_count: usize,
}

impl JiraClient {
    pub async fn get_issue_dates(
        &self,
        issue_key: &str,
        include_status_changes: bool,
        include_status_summary: bool,
    ) -> anyhow::Result<Value> {
        self.ensure_issue_allowed(issue_key)?;
        let expand = if include_status_changes || include_status_summary {
            Some("changelog".to_string())
        } else {
            None
        };
        let mut query = vec![(
            "fields",
            "status,created,updated,duedate,resolutiondate".to_string(),
        )];
        if let Some(expand) = expand {
            query.push(("expand", expand));
        }
        let issue = self
            .client
            .get_json(&format!("rest/api/3/issue/{issue_key}"), &query)
            .await?;
        let fields = issue.get("fields").unwrap_or(&Value::Null);
        let created = fields.get("created").and_then(parse_datetime);
        let updated = fields.get("updated").and_then(parse_datetime);
        let due_date = fields.get("duedate").and_then(parse_datetime);
        let resolution_date = fields.get("resolutiondate").and_then(parse_datetime);
        let current_status = fields
            .get("status")
            .and_then(|status| value_str(status, "name"))
            .map(ToOwned::to_owned);

        let now = Utc::now();
        let mut changes = Vec::new();
        let mut summary = Vec::new();
        if include_status_changes || include_status_summary {
            changes = parse_status_changes(&issue, created);
            if include_status_summary {
                summary = aggregate_statuses(&changes, now);
            }
        }
        let mut out = serde_json::Map::new();
        out.insert("issue_key".into(), Value::String(issue_key.to_string()));
        if let Some(created) = created {
            out.insert("created".into(), Value::String(created.to_rfc3339()));
        }
        if let Some(updated) = updated {
            out.insert("updated".into(), Value::String(updated.to_rfc3339()));
        }
        if let Some(due_date) = due_date {
            out.insert("due_date".into(), Value::String(due_date.to_rfc3339()));
        }
        if let Some(resolution_date) = resolution_date {
            out.insert(
                "resolution_date".into(),
                Value::String(resolution_date.to_rfc3339()),
            );
        }
        if let Some(current_status) = current_status {
            out.insert("current_status".into(), Value::String(current_status));
        }
        if include_status_changes && !changes.is_empty() {
            out.insert(
                "status_changes".into(),
                Value::Array(
                    changes
                        .iter()
                        .map(|change| {
                            let minutes = change.duration_minutes(now);
                            let mut entry = serde_json::Map::new();
                            entry.insert("status".into(), Value::String(change.status.clone()));
                            entry.insert(
                                "entered_at".into(),
                                Value::String(change.entered_at.to_rfc3339()),
                            );
                            if let Some(exited_at) = change.exited_at {
                                entry.insert(
                                    "exited_at".into(),
                                    Value::String(exited_at.to_rfc3339()),
                                );
                            }
                            entry.insert("duration_minutes".into(), json!(minutes));
                            entry.insert(
                                "duration_formatted".into(),
                                Value::String(format_duration(minutes)),
                            );
                            if let Some(by) = &change.transitioned_by {
                                entry.insert("transitioned_by".into(), Value::String(by.clone()));
                            }
                            Value::Object(entry)
                        })
                        .collect(),
                ),
            );
        }
        if include_status_summary && !summary.is_empty() {
            out.insert(
                "status_summary".into(),
                Value::Array(
                    summary
                        .iter()
                        .map(|entry| {
                            json!({
                                "status": entry.status,
                                "total_duration_minutes": entry.total_minutes,
                                "total_duration_formatted": format_duration(entry.total_minutes),
                                "visit_count": entry.visit_count,
                            })
                        })
                        .collect(),
                ),
            );
        }
        Ok(Value::Object(out))
    }
}

pub fn parse_datetime(value: &Value) -> Option<DateTime<Utc>> {
    let text = value.as_str()?;
    parse_datetime_str(text)
}

pub fn parse_datetime_str(text: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Some(parsed.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.3f%z", "%Y-%m-%dT%H:%M:%S%z"] {
        if let Ok(parsed) = DateTime::parse_from_str(text, format) {
            return Some(parsed.with_timezone(&Utc));
        }
    }
    if let Ok(parsed) = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Some(parsed.and_hms_opt(0, 0, 0)?.and_utc());
    }
    None
}

fn parse_status_changes(issue: &Value, created: Option<DateTime<Utc>>) -> Vec<StatusChange> {
    let mut histories: Vec<&Value> = issue
        .get("changelog")
        .and_then(|changelog| changelog.get("histories"))
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default();
    histories.sort_by_key(|history| {
        history
            .get("created")
            .and_then(parse_datetime)
            .map(|date| date.timestamp_millis())
            .unwrap_or_default()
    });

    type StatusTransition = (Option<String>, String, DateTime<Utc>, Option<String>);
    let mut transitions: Vec<StatusTransition> = Vec::new();
    for history in histories {
        let Some(changed_at) = history.get("created").and_then(parse_datetime) else {
            continue;
        };
        let author = history
            .get("author")
            .and_then(|author| author.get("displayName"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        if let Some(items) = history.get("items").and_then(Value::as_array) {
            for item in items {
                if item.get("field").and_then(Value::as_str) != Some("status") {
                    continue;
                }
                let from = item
                    .get("fromString")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let to = item
                    .get("toString")
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown")
                    .to_string();
                transitions.push((from, to, changed_at, author.clone()));
            }
        }
    }
    transitions.sort_by_key(|(_, _, at, _)| at.timestamp_millis());
    if transitions.is_empty() {
        return Vec::new();
    }

    let mut changes = Vec::new();
    if let Some(created) = created
        && let Some(from) = transitions[0].0.as_ref()
    {
        changes.push(StatusChange {
            status: from.clone(),
            entered_at: created,
            exited_at: Some(transitions[0].2),
            transitioned_by: None,
        });
    }
    for (index, (_, to, entered_at, author)) in transitions.iter().enumerate() {
        changes.push(StatusChange {
            status: to.clone(),
            entered_at: *entered_at,
            exited_at: transitions.get(index + 1).map(|(_, _, at, _)| *at),
            transitioned_by: author.clone(),
        });
    }
    changes
}

fn aggregate_statuses(changes: &[StatusChange], now: DateTime<Utc>) -> Vec<StatusSummary> {
    let mut map: Vec<StatusSummary> = Vec::new();
    for change in changes {
        let minutes = change.duration_minutes(now);
        if let Some(entry) = map.iter_mut().find(|entry| entry.status == change.status) {
            entry.total_minutes += minutes;
            entry.visit_count += 1;
        } else {
            map.push(StatusSummary {
                status: change.status.clone(),
                total_minutes: minutes,
                visit_count: 1,
            });
        }
    }
    map.sort_by_key(|a| std::cmp::Reverse(a.total_minutes));
    map
}

pub fn format_duration(minutes: i64) -> String {
    if minutes <= 0 {
        return "0m".to_string();
    }
    let days = minutes / (24 * 60);
    let hours = (minutes % (24 * 60)) / 60;
    let mins = minutes % 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if mins > 0 || parts.is_empty() {
        parts.push(format!("{mins}m"));
    }
    parts.join(" ")
}
