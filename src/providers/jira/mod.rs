pub mod agile;
pub mod attachments;
pub mod dates;
pub mod development;
pub mod fields;
pub mod issue;
pub mod links;
pub mod proforma;
pub mod projects;
pub mod service_desk;
pub mod sla;
pub mod transitions;
pub mod users;
pub mod watchers;
pub mod worklog;

use serde_json::{Map, Value, json};

use crate::config::AtlassianConfig;
use crate::markdown::jira_rich_text;
use crate::model::value_str;
use crate::providers::AtlassianClient;

pub const DEFAULT_READ_FIELDS: &str =
    "updated,reporter,assignee,description,status,labels,summary,created,priority,issuetype";

#[derive(Clone)]
pub struct JiraClient {
    pub(crate) client: AtlassianClient,
    pub(crate) projects_filter: Vec<String>,
}

impl JiraClient {
    pub fn new(
        cfg: &AtlassianConfig,
        http: reqwest::Client,
        retry_max: usize,
        projects_filter: Vec<String>,
    ) -> Self {
        Self {
            client: AtlassianClient::new(cfg, crate::model::Product::Jira, http, retry_max),
            projects_filter,
        }
    }

    pub(crate) fn ensure_project_allowed(&self, project_key: &str) -> anyhow::Result<()> {
        if !self.projects_filter.is_empty()
            && !self
                .projects_filter
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(project_key))
        {
            anyhow::bail!("project '{project_key}' is excluded by JIRA_PROJECTS_FILTER");
        }
        Ok(())
    }

    pub(crate) fn ensure_issue_allowed(&self, issue_key: &str) -> anyhow::Result<()> {
        let project = issue_key
            .split_once('-')
            .map(|(project, _)| project)
            .unwrap_or(issue_key);
        self.ensure_project_allowed(project)
    }
}

pub fn parse_fields(fields: Option<&str>) -> Vec<String> {
    match fields {
        None => DEFAULT_READ_FIELDS
            .split(',')
            .map(ToOwned::to_owned)
            .collect(),
        Some("*all") => vec!["*all".to_string()],
        Some(raw) => raw
            .split(',')
            .map(str::trim)
            .filter(|field| !field.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
    }
}

pub fn fields_query(fields: &[String]) -> String {
    if fields.len() == 1 && fields[0] == "*all" {
        "*all".to_string()
    } else {
        fields.join(",")
    }
}

pub fn simplify_user(value: &Value) -> Value {
    if value.is_null() {
        return json!({"display_name": "Unassigned", "name": "Unassigned", "email": null, "avatar_url": null});
    }
    let avatar = value
        .get("avatarUrls")
        .and_then(|urls| urls.get("48x48"))
        .and_then(Value::as_str);
    let mut map = Map::new();
    map.insert(
        "display_name".into(),
        Value::String(
            value_str(value, "displayName")
                .unwrap_or("Unassigned")
                .to_string(),
        ),
    );
    map.insert(
        "name".into(),
        Value::String(
            value_str(value, "name")
                .or_else(|| value_str(value, "displayName"))
                .unwrap_or("Unassigned")
                .to_string(),
        ),
    );
    map.insert(
        "email".into(),
        value.get("emailAddress").cloned().unwrap_or(Value::Null),
    );
    map.insert(
        "avatar_url".into(),
        avatar
            .map(|v| Value::String(v.to_string()))
            .unwrap_or(Value::Null),
    );
    if let Some(key) = value.get("key").and_then(Value::as_str) {
        map.insert("key".into(), Value::String(key.to_string()));
    }
    if let Some(account_id) = value.get("accountId").and_then(Value::as_str) {
        map.insert("account_id".into(), Value::String(account_id.to_string()));
    }
    if let Some(active) = value.get("active").and_then(Value::as_bool) {
        map.insert("active".into(), Value::Bool(active));
    }
    if let Some(zone) = value.get("timeZone").and_then(Value::as_str) {
        map.insert("time_zone".into(), Value::String(zone.to_string()));
    }
    Value::Object(map)
}

pub fn simplify_status(value: &Value) -> Value {
    let mut map = Map::new();
    map.insert(
        "name".into(),
        Value::String(value_str(value, "name").unwrap_or("Unknown").to_string()),
    );
    if let Some(category) = value.get("statusCategory") {
        if let Some(name) = value_str(category, "name") {
            map.insert("category".into(), Value::String(name.to_string()));
        }
        if let Some(color) = value_str(category, "colorName") {
            map.insert("color".into(), Value::String(color.to_string()));
        }
    }
    Value::Object(map)
}

pub fn simplify_issue_type(value: &Value) -> Value {
    let mut map = Map::new();
    map.insert(
        "name".into(),
        Value::String(value_str(value, "name").unwrap_or("Unknown").to_string()),
    );
    if let Some(icon) = value_str(value, "iconUrl") {
        map.insert("icon_url".into(), Value::String(icon.to_string()));
    }
    if let Some(id) = value.get("id").and_then(Value::as_str) {
        map.insert("id".into(), Value::String(id.to_string()));
    }
    Value::Object(map)
}

pub fn simplify_priority(value: &Value) -> Value {
    let mut map = Map::new();
    map.insert(
        "name".into(),
        Value::String(value_str(value, "name").unwrap_or("Unknown").to_string()),
    );
    if let Some(icon) = value_str(value, "iconUrl") {
        map.insert("icon_url".into(), Value::String(icon.to_string()));
    }
    Value::Object(map)
}

pub fn simplify_project(value: &Value) -> Value {
    let mut map = Map::new();
    map.insert(
        "key".into(),
        Value::String(value_str(value, "key").unwrap_or_default().to_string()),
    );
    map.insert(
        "name".into(),
        Value::String(value_str(value, "name").unwrap_or_default().to_string()),
    );
    if let Some(id) = value.get("id").and_then(Value::as_str) {
        map.insert("id".into(), Value::String(id.to_string()));
    }
    Value::Object(map)
}

pub fn simplify_comment(value: &Value) -> Value {
    let mut map = Map::new();
    map.insert(
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
    map.insert(
        "body".into(),
        Value::String(jira_rich_text(value.get("body").unwrap_or(&Value::Null))),
    );
    if let Some(author) = value.get("author") {
        map.insert("author".into(), simplify_user(author));
    }
    if let Some(created) = value_str(value, "created") {
        map.insert("created".into(), Value::String(created.to_string()));
    }
    if let Some(updated) = value_str(value, "updated") {
        map.insert("updated".into(), Value::String(updated.to_string()));
    }
    Value::Object(map)
}

pub fn simplify_attachment(value: &Value) -> Value {
    crate::model::Attachment::from_jira(value)
        .map(|attachment| attachment.to_json())
        .unwrap_or_else(|| value.clone())
}

fn should_include(fields: &[String], name: &str) -> bool {
    fields.iter().any(|field| field == "*all" || field == name)
}

fn nested<'a>(fields: &'a Value, name: &str) -> Option<&'a Value> {
    fields.get(name).filter(|value| !value.is_null())
}

/// Build the same compact issue shape that mcp-atlassian returns.
pub fn simplify_issue(value: &Value, requested_fields: &[String]) -> Value {
    let fields = value.get("fields").unwrap_or(&Value::Null);
    let mut out = Map::new();
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
        "key".into(),
        Value::String(value_str(value, "key").unwrap_or_default().to_string()),
    );

    if should_include(requested_fields, "summary")
        && let Some(summary) = value_str(fields, "summary")
    {
        out.insert("summary".into(), Value::String(summary.to_string()));
    }
    if should_include(requested_fields, "url")
        && let Some(url) = value_str(value, "self")
    {
        out.insert("url".into(), Value::String(url.to_string()));
    }
    if should_include(requested_fields, "description") {
        let description = fields.get("description").unwrap_or(&Value::Null);
        let text = jira_rich_text(description);
        if !text.is_empty() {
            out.insert("description".into(), Value::String(text));
        }
    }
    if should_include(requested_fields, "status")
        && let Some(status) = nested(fields, "status")
    {
        out.insert("status".into(), simplify_status(status));
    }
    if should_include(requested_fields, "issuetype")
        && let Some(issue_type) = nested(fields, "issuetype")
    {
        out.insert("issue_type".into(), simplify_issue_type(issue_type));
    }
    if should_include(requested_fields, "priority")
        && let Some(priority) = nested(fields, "priority")
    {
        out.insert("priority".into(), simplify_priority(priority));
    }
    if should_include(requested_fields, "project")
        && let Some(project) = nested(fields, "project")
    {
        out.insert("project".into(), simplify_project(project));
    }
    if should_include(requested_fields, "resolution")
        && let Some(resolution) = nested(fields, "resolution")
    {
        out.insert(
            "resolution".into(),
            json!({"name": value_str(resolution, "name").unwrap_or("Done")}),
        );
    }
    for name in ["duedate", "resolutiondate"] {
        if should_include(requested_fields, name)
            && let Some(value) = fields.get(name).filter(|v| !v.is_null())
        {
            out.insert(name.into(), value.clone());
        }
    }
    if should_include(requested_fields, "parent")
        && let Some(parent) = nested(fields, "parent")
    {
        out.insert("parent".into(), parent.clone());
    }
    if should_include(requested_fields, "subtasks")
        && let Some(subtasks) = fields.get("subtasks").and_then(Value::as_array)
    {
        let items: Vec<Value> = subtasks
            .iter()
            .map(|task| {
                json!({
                    "key": value_str(task, "key"),
                    "summary": task.get("fields").and_then(|f| f.get("summary")).cloned(),
                })
            })
            .collect();
        out.insert("subtasks".into(), Value::Array(items));
    }
    if should_include(requested_fields, "security")
        && let Some(security) = nested(fields, "security")
    {
        out.insert("security".into(), security.clone());
    }
    if should_include(requested_fields, "worklog")
        && let Some(worklog) = nested(fields, "worklog")
    {
        out.insert("worklog".into(), worklog.clone());
    }
    if should_include(requested_fields, "assignee") {
        out.insert(
            "assignee".into(),
            simplify_user(fields.get("assignee").unwrap_or(&Value::Null)),
        );
    }
    if should_include(requested_fields, "reporter")
        && let Some(reporter) = nested(fields, "reporter")
    {
        out.insert("reporter".into(), simplify_user(reporter));
    }
    for (name, output) in [
        ("labels", "labels"),
        ("components", "components"),
        ("fixVersions", "fix_versions"),
    ] {
        if should_include(requested_fields, name)
            && let Some(values) = fields.get(name).and_then(Value::as_array)
        {
            let values: Vec<Value> = values
                .iter()
                .map(|item| {
                    if let Some(name) = value_str(item, "name") {
                        Value::String(name.to_string())
                    } else {
                        item.clone()
                    }
                })
                .collect();
            out.insert(output.into(), Value::Array(values));
        }
    }
    for (name, output) in [("issuelinks", "issuelinks"), ("attachment", "attachments")] {
        if should_include(requested_fields, name)
            && let Some(values) = fields.get(name).and_then(Value::as_array)
        {
            let values: Vec<Value> = values
                .iter()
                .map(|item| {
                    if name == "attachment" {
                        simplify_attachment(item)
                    } else {
                        item.clone()
                    }
                })
                .collect();
            out.insert(output.into(), Value::Array(values));
        }
    }
    if should_include(requested_fields, "comment")
        && let Some(comments) = fields
            .get("comment")
            .and_then(|comment| comment.get("comments"))
            .and_then(Value::as_array)
    {
        let comments: Vec<Value> = comments.iter().map(simplify_comment).collect();
        if !comments.is_empty() {
            out.insert("comments".into(), Value::Array(comments));
        }
    }
    if let Some(changelog) = value
        .get("changelog")
        .and_then(|changelog| changelog.get("histories"))
        .and_then(Value::as_array)
    {
        out.insert("changelogs".into(), Value::Array(changelog.clone()));
    }
    for (name, output) in [("created", "created"), ("updated", "updated")] {
        if should_include(requested_fields, name)
            && let Some(timestamp) = value_str(fields, name)
        {
            out.insert(output.into(), Value::String(timestamp.to_string()));
        }
    }
    if let Some(epic_key) = value_str(fields, "epic_key") {
        out.insert("epic_key".into(), Value::String(epic_key.to_string()));
    }
    if let Some(epic_name) = value_str(fields, "epic_name") {
        out.insert("epic_name".into(), Value::String(epic_name.to_string()));
    }
    if should_include(requested_fields, "timetracking")
        && let Some(timetracking) = nested(fields, "timetracking")
    {
        out.insert("timetracking".into(), timetracking.clone());
    }
    if requested_fields.iter().any(|field| field == "*all") {
        let names = value.get("names").and_then(Value::as_object);
        for (field_id, field_value) in fields.as_object().into_iter().flatten() {
            if !field_id.starts_with("customfield_") || out.contains_key(field_id) {
                continue;
            }
            let mut entry = Map::new();
            entry.insert("value".into(), field_value.clone());
            if let Some(name) = names
                .and_then(|names| names.get(field_id))
                .and_then(Value::as_str)
            {
                entry.insert("name".into(), Value::String(name.to_string()));
            }
            out.insert(field_id.clone(), Value::Object(entry));
        }
    }
    Value::Object(out)
}
