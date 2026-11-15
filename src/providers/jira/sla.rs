use std::collections::HashMap;

use chrono::{DateTime, Datelike, NaiveTime, Utc};
use serde_json::{Map, Value, json};

use crate::model::value_str;
use crate::providers::jira::JiraClient;
use crate::providers::jira::dates::{format_duration, parse_datetime_str};

#[derive(Clone, Debug)]
struct Change {
    status: String,
    entered: DateTime<Utc>,
    exited: Option<DateTime<Utc>>,
    transitioned_by: Option<String>,
}

impl JiraClient {
    pub async fn get_issue_sla(
        &self,
        issue_key: &str,
        metrics: Option<&[String]>,
        working_hours_only: Option<bool>,
        include_raw_dates: bool,
    ) -> anyhow::Result<Value> {
        let working =
            working_hours_only.unwrap_or_else(|| env_bool("JIRA_SLA_WORKING_HOURS_ONLY", false));
        let dates = self.get_issue_dates(issue_key, true, true).await?;
        let created = dates
            .get("created")
            .and_then(Value::as_str)
            .and_then(parse_datetime_str);
        let updated = dates
            .get("updated")
            .and_then(Value::as_str)
            .and_then(parse_datetime_str);
        let due_date = dates
            .get("due_date")
            .and_then(Value::as_str)
            .and_then(parse_datetime_str);
        let resolution_date = dates
            .get("resolution_date")
            .and_then(Value::as_str)
            .and_then(parse_datetime_str);
        let current_status = dates
            .get("current_status")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let changes = parse_changes(&dates);
        let now = Utc::now();
        let requested: Vec<String> = metrics
            .map(|list| list.iter().map(|m| m.trim().to_ascii_lowercase()).collect())
            .unwrap_or_else(|| vec!["cycle_time".to_string(), "time_in_status".to_string()]);

        let mut output = Map::new();
        if requested.iter().any(|m| m == "cycle_time") {
            output.insert(
                "cycle_time".into(),
                match (created, resolution_date) {
                    (Some(start), Some(end)) => {
                        let minutes = duration_minutes(start, end, working);
                        json!({
                            "calculated": true,
                            "value_minutes": minutes,
                            "formatted": format_duration(minutes),
                        })
                    }
                    _ => json!({"calculated": false, "reason": "issue is not resolved"}),
                },
            );
        }
        if requested.iter().any(|m| m == "lead_time")
            && let Some(start) = created
        {
            let end = resolution_date.unwrap_or(now);
            let minutes = duration_minutes(start, end, working);
            output.insert(
                "lead_time".into(),
                json!({
                    "value_minutes": minutes,
                    "formatted": format_duration(minutes),
                    "is_resolved": resolution_date.is_some(),
                }),
            );
        }
        if requested.iter().any(|m| m == "time_in_status") {
            let statuses = if working {
                aggregate_working(&changes, now, working)
            } else {
                aggregate_summary(&dates)
            };
            let total: i64 = statuses.iter().map(|(_, minutes, _)| *minutes).sum();
            let rows: Vec<Value> = statuses
                .iter()
                .map(|(status, minutes, visits)| {
                    let percentage = if total > 0 {
                        (*minutes as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    };
                    json!({
                        "status": status,
                        "value_minutes": minutes,
                        "formatted": format_duration(*minutes),
                        "percentage": (percentage * 100.0).round() / 100.0,
                        "visit_count": visits,
                    })
                })
                .collect();
            output.insert(
                "time_in_status".into(),
                json!({"statuses": rows, "total_minutes": total}),
            );
        }
        if requested.iter().any(|m| m == "due_date_compliance") {
            let compliance = match (due_date, resolution_date) {
                (None, _) => json!({"status": "no_due_date"}),
                (Some(_), None) => json!({"status": "not_resolved"}),
                (Some(due), Some(resolved)) => {
                    let margin = (due - resolved).num_minutes();
                    json!({
                        "status": if margin >= 0 { "met" } else { "missed" },
                        "margin_minutes": margin,
                        "formatted_margin": format_duration(margin.abs()),
                    })
                }
            };
            output.insert("due_date_compliance".into(), compliance);
        }
        if requested.iter().any(|m| m == "resolution_time") {
            let categories = self.status_categories().await.unwrap_or_default();
            let first_progress = changes
                .iter()
                .find(|change| {
                    categories
                        .get(&change.status)
                        .is_some_and(|category| category == "indeterminate")
                        || change.status.to_ascii_lowercase().contains("progress")
                        || change.status.to_ascii_lowercase().contains("review")
                })
                .map(|change| change.entered);
            output.insert(
                "resolution_time".into(),
                match (first_progress, resolution_date) {
                    (Some(start), Some(end)) => {
                        let minutes = duration_minutes(start, end, working);
                        json!({
                            "calculated": true,
                            "value_minutes": minutes,
                            "formatted": format_duration(minutes),
                        })
                    }
                    _ => json!({"calculated": false, "reason": "no in-progress transition or not resolved"}),
                },
            );
        }
        if requested.iter().any(|m| m == "first_response_time") {
            let first_transition = changes
                .iter()
                .find(|change| change.transitioned_by.is_some())
                .map(|change| change.entered);
            let first_comment = self.first_comment_date(issue_key).await.ok().flatten();
            let response = match (first_transition, first_comment) {
                (Some(transition), Some(comment)) if transition <= comment => {
                    Some((transition, "transition"))
                }
                (_, Some(comment)) => Some((comment, "comment")),
                (Some(transition), None) => Some((transition, "transition")),
                _ => None,
            };
            output.insert(
                "first_response_time".into(),
                match (created, response) {
                    (Some(start), Some((response_at, kind))) => {
                        let minutes = duration_minutes(start, response_at, working);
                        json!({
                            "calculated": true,
                            "value_minutes": minutes,
                            "formatted": format_duration(minutes),
                            "response_type": kind,
                        })
                    }
                    _ => json!({"calculated": false}),
                },
            );
        }

        let mut result = Map::new();
        result.insert("issue_key".into(), Value::String(issue_key.to_string()));
        result.insert("metrics".into(), Value::Object(output));
        if include_raw_dates {
            let mut raw = Map::new();
            if let Some(created) = created {
                raw.insert("created".into(), Value::String(created.to_rfc3339()));
            }
            if let Some(updated) = updated {
                raw.insert("updated".into(), Value::String(updated.to_rfc3339()));
            }
            if let Some(due_date) = due_date {
                raw.insert("due_date".into(), Value::String(due_date.to_rfc3339()));
            }
            if let Some(resolution_date) = resolution_date {
                raw.insert(
                    "resolution_date".into(),
                    Value::String(resolution_date.to_rfc3339()),
                );
            }
            if let Some(current_status) = current_status {
                raw.insert("current_status".into(), Value::String(current_status));
            }
            if let Some(changes) = dates.get("status_changes") {
                raw.insert("status_changes".into(), changes.clone());
            }
            result.insert("raw_dates".into(), Value::Object(raw));
        }
        Ok(Value::Object(result))
    }

    async fn first_comment_date(&self, issue_key: &str) -> anyhow::Result<Option<DateTime<Utc>>> {
        let response = self
            .client
            .get_json(
                &format!("rest/api/3/issue/{issue_key}/comment"),
                &[
                    ("maxResults", "1".to_string()),
                    ("orderBy", "+created".to_string()),
                ],
            )
            .await?;
        Ok(response
            .get("comments")
            .and_then(Value::as_array)
            .and_then(|comments| comments.first())
            .and_then(|comment| comment.get("created"))
            .and_then(Value::as_str)
            .and_then(parse_datetime_str))
    }

    async fn status_categories(&self) -> anyhow::Result<HashMap<String, String>> {
        let response = self.client.get_json("rest/api/3/status", &[]).await?;
        let mut map = HashMap::new();
        if let Some(statuses) = response.as_array() {
            for status in statuses {
                if let (Some(name), Some(category)) = (
                    status.get("name").and_then(Value::as_str),
                    status
                        .get("statusCategory")
                        .and_then(|category| category.get("key"))
                        .and_then(Value::as_str),
                ) {
                    map.insert(name.to_string(), category.to_string());
                }
            }
        }
        Ok(map)
    }
}

fn parse_changes(dates: &Value) -> Vec<Change> {
    dates
        .get("status_changes")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let status = value_str(item, "status")?.to_string();
                    let entered = item
                        .get("entered_at")
                        .and_then(Value::as_str)
                        .and_then(parse_datetime_str)?;
                    let exited = item
                        .get("exited_at")
                        .and_then(Value::as_str)
                        .and_then(parse_datetime_str);
                    let transitioned_by = item
                        .get("transitioned_by")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                    Some(Change {
                        status,
                        entered,
                        exited,
                        transitioned_by,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn aggregate_summary(dates: &Value) -> Vec<(String, i64, usize)> {
    dates
        .get("status_summary")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some((
                        value_str(item, "status")?.to_string(),
                        item.get("total_duration_minutes")
                            .and_then(Value::as_i64)
                            .unwrap_or(0),
                        item.get("visit_count").and_then(Value::as_u64).unwrap_or(1) as usize,
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn aggregate_working(
    changes: &[Change],
    now: DateTime<Utc>,
    working: bool,
) -> Vec<(String, i64, usize)> {
    let mut result: Vec<(String, i64, usize)> = Vec::new();
    for change in changes {
        let end = change.exited.unwrap_or(now);
        let minutes = duration_minutes(change.entered, end, working);
        if let Some(entry) = result.iter_mut().find(|entry| entry.0 == change.status) {
            entry.1 += minutes;
            entry.2 += 1;
        } else {
            result.push((change.status.clone(), minutes, 1));
        }
    }
    result.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    result
}

fn duration_minutes(start: DateTime<Utc>, end: DateTime<Utc>, working: bool) -> i64 {
    if end <= start {
        return 0;
    }
    if !working {
        return (end - start).num_minutes().max(0);
    }
    let start_time = env_time("JIRA_SLA_WORKING_HOURS_START", "09:00");
    let end_time = env_time("JIRA_SLA_WORKING_HOURS_END", "17:00");
    if end_time <= start_time {
        return (end - start).num_minutes().max(0);
    }
    let days = env_days("JIRA_SLA_WORKING_DAYS");
    let mut total = 0i64;
    let mut day = start.date_naive();
    let last = end.date_naive();
    while day <= last {
        if days.contains(&day.weekday().number_from_monday()) {
            let day_start = day.and_time(start_time).and_utc();
            let day_end = day.and_time(end_time).and_utc();
            let overlap_start = start.max(day_start);
            let overlap_end = end.min(day_end);
            if overlap_end > overlap_start {
                total += (overlap_end - overlap_start).num_minutes();
            }
        }
        let Some(next) = day.succ_opt() else {
            break;
        };
        day = next;
    }
    total
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn env_time(name: &str, default: &str) -> NaiveTime {
    std::env::var(name)
        .ok()
        .and_then(|value| NaiveTime::parse_from_str(value.trim(), "%H:%M").ok())
        .unwrap_or_else(|| NaiveTime::parse_from_str(default, "%H:%M").expect("valid default time"))
}

fn env_days(name: &str) -> Vec<u32> {
    let raw = std::env::var(name).unwrap_or_else(|_| "1,2,3,4,5".to_string());
    let days: Vec<u32> = raw
        .split(',')
        .filter_map(|value| value.trim().parse().ok())
        .filter(|day| (1..=7).contains(day))
        .collect();
    if days.is_empty() {
        vec![1, 2, 3, 4, 5]
    } else {
        days
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_clock_duration() {
        let start = parse_datetime_str("2026-01-01T09:00:00Z").unwrap();
        let end = parse_datetime_str("2026-01-01T11:30:00Z").unwrap();
        assert_eq!(duration_minutes(start, end, false), 150);
    }

    #[test]
    fn working_duration_skips_non_working_hours() {
        // 2026-01-01 is Thursday; 09:00-17:00 working day.
        let start = parse_datetime_str("2026-01-01T08:00:00Z").unwrap();
        let end = parse_datetime_str("2026-01-01T18:00:00Z").unwrap();
        assert_eq!(duration_minutes(start, end, true), 8 * 60);
    }
}
