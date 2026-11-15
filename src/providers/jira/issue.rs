use serde_json::{Value, json};

use crate::model::value_str;
use crate::providers::jira::{
    JiraClient, fields_query, parse_fields, simplify_issue, simplify_user,
};

impl JiraClient {
    pub async fn get_issue(
        &self,
        issue_key: &str,
        fields: Option<&str>,
        expand: Option<&str>,
        comment_limit: u32,
        properties: Option<&str>,
    ) -> anyhow::Result<Value> {
        self.ensure_issue_allowed(issue_key)?;
        let mut requested = parse_fields(fields);
        if requested.len() == 1 && requested[0] == "*all" {
            requested = vec!["*all".to_string()];
        } else if comment_limit > 0 && !requested.iter().any(|field| field == "comment") {
            requested.push("comment".to_string());
        }
        let mut query = vec![("fields", fields_query(&requested))];
        if let Some(expand) = expand.filter(|v| !v.is_empty()) {
            query.push(("expand", expand.to_string()));
        }
        if let Some(properties) = properties.filter(|v| !v.is_empty()) {
            query.push(("properties", properties.to_string()));
        }
        let mut issue = self
            .client
            .get_json(&format!("rest/api/3/issue/{issue_key}"), &query)
            .await?;

        if comment_limit > 0 {
            let comments = self
                .client
                .get_json(
                    &format!("rest/api/3/issue/{issue_key}/comment"),
                    &[
                        ("maxResults", comment_limit.to_string()),
                        ("orderBy", "+created".to_string()),
                    ],
                )
                .await?;
            if let Some(object) = issue.as_object_mut() {
                let fields_value = object.entry("fields").or_insert_with(|| json!({}));
                if let Some(fields_object) = fields_value.as_object_mut() {
                    let comment_value = fields_object.entry("comment").or_insert_with(|| json!({}));
                    if let Some(comment_object) = comment_value.as_object_mut() {
                        comment_object.insert(
                            "comments".into(),
                            comments.get("comments").cloned().unwrap_or(json!([])),
                        );
                    }
                }
            }
        }
        Ok(simplify_issue(&issue, &requested))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn search(
        &self,
        jql: &str,
        fields: Option<&str>,
        limit: u32,
        start_at: u32,
        expand: Option<&str>,
        projects_filter: Option<&str>,
        page_token: Option<&str>,
    ) -> anyhow::Result<Value> {
        let filter = projects_filter
            .map(str::to_string)
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| self.projects_filter.join(","));
        let jql = apply_projects_filter(jql, &filter);
        let requested = parse_fields(fields);
        let fields_list: Vec<String> = if requested.len() == 1 && requested[0] == "*all" {
            vec!["*all".to_string()]
        } else {
            requested.clone()
        };
        let limit = limit.clamp(1, 50) as usize;
        let skip = (start_at as usize).min(1000);

        let mut issues = Vec::new();
        let mut next_token = page_token.map(ToOwned::to_owned);
        let mut fetched = 0usize;
        let mut loops = 0;
        while issues.len() < limit && loops < 20 {
            loops += 1;
            let remaining = (limit + skip).saturating_sub(fetched);
            if remaining == 0 {
                break;
            }
            let mut body = json!({
                "jql": jql,
                "fields": fields_list,
                "maxResults": remaining.min(100),
            });
            if let Some(expand) = expand.filter(|v| !v.is_empty()) {
                body["expand"] = Value::String(expand.to_string());
            }
            if let Some(token) = next_token.as_deref() {
                body["nextPageToken"] = Value::String(token.to_string());
            }
            let response = self
                .client
                .post_json("rest/api/3/search/jql", &body)
                .await?;
            let page = response
                .get("issues")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if page.is_empty() {
                break;
            }
            let page_len = page.len();
            let page_start = fetched;
            fetched += page_len;
            for (offset, issue) in page.into_iter().enumerate() {
                if issues.len() >= limit {
                    break;
                }
                if page_start + offset < skip {
                    continue;
                }
                issues.push(simplify_issue(&issue, &requested));
            }
            next_token = response
                .get("nextPageToken")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            if next_token.is_none() {
                break;
            }
        }
        let mut result = json!({
            "total": -1,
            "start_at": start_at,
            "max_results": limit,
            "issues": issues,
        });
        if let Some(token) = next_token {
            result["next_page_token"] = Value::String(token);
        }
        Ok(result)
    }

    pub async fn get_project_issues(
        &self,
        project_key: &str,
        limit: u32,
        start_at: u32,
    ) -> anyhow::Result<Value> {
        self.ensure_project_allowed(project_key)?;
        let jql = format!(
            "project = \"{}\" ORDER BY created DESC",
            escape_jql(project_key)
        );
        self.search(&jql, None, limit, start_at, None, Some(project_key), None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn get_board_issues(
        &self,
        board_id: &str,
        jql: &str,
        fields: Option<&str>,
        start_at: u32,
        limit: u32,
        expand: Option<&str>,
    ) -> anyhow::Result<Value> {
        let requested = parse_fields(fields);
        let mut query = vec![
            ("jql", jql.to_string()),
            ("fields", fields_query(&requested)),
            ("startAt", start_at.to_string()),
            ("maxResults", limit.clamp(1, 50).to_string()),
        ];
        if let Some(expand) = expand.filter(|v| !v.is_empty()) {
            query.push(("expand", expand.to_string()));
        }
        let response = self
            .client
            .get_json(&format!("rest/agile/1.0/board/{board_id}/issue"), &query)
            .await?;
        let issues = response
            .get("issues")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|issue| simplify_issue(issue, &requested))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(json!({
            "total": response.get("total").cloned().unwrap_or(json!(issues.len())),
            "start_at": response.get("startAt").cloned().unwrap_or(json!(start_at)),
            "max_results": response.get("maxResults").cloned().unwrap_or(json!(limit)),
            "issues": issues,
        }))
    }

    pub async fn get_sprint_issues(
        &self,
        sprint_id: &str,
        fields: Option<&str>,
        start_at: u32,
        limit: u32,
    ) -> anyhow::Result<Value> {
        let jql = format!("sprint = {sprint_id}");
        self.search(&jql, fields, limit, start_at, None, None, None)
            .await
    }

    pub async fn batch_get_changelogs(
        &self,
        issue_keys: &[String],
        fields: Option<&[String]>,
        limit: i64,
    ) -> anyhow::Result<Value> {
        let mut all: Vec<Value> = Vec::new();
        let mut next_token: Option<String> = None;
        let mut loops = 0;
        loop {
            loops += 1;
            if loops > 100 {
                break;
            }
            let mut body = json!({
                "issueIdsOrKeys": issue_keys,
            });
            if let Some(fields) = fields {
                body["fieldIds"] = json!(fields);
            }
            if let Some(token) = next_token.as_deref() {
                body["nextPageToken"] = Value::String(token.to_string());
            }
            let response = self
                .client
                .post_json("rest/api/3/changelog/bulkfetch", &body)
                .await?;
            if let Some(logs) = response.get("issueChangeLogs").and_then(Value::as_array) {
                all.extend(logs.iter().cloned());
            }
            next_token = response
                .get("nextPageToken")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            if next_token.is_none() {
                break;
            }
        }
        let results = all
            .iter()
            .map(|entry| {
                let issue_id = value_str(entry, "issueId").unwrap_or_default();
                let mut changelogs: Vec<Value> = entry
                    .get("changeHistories")
                    .and_then(Value::as_array)
                    .map(|items| items.iter().map(simplify_changelog).collect::<Vec<_>>())
                    .unwrap_or_default();
                if limit >= 0 {
                    changelogs.truncate(limit as usize);
                }
                json!({"issue_id": issue_id, "changelogs": changelogs})
            })
            .collect::<Vec<_>>();
        Ok(Value::Array(results))
    }
}

pub fn simplify_changelog(value: &Value) -> Value {
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let mut out = serde_json::Map::new();
                    for (source, target) in [
                        ("field", "field"),
                        ("fieldtype", "fieldtype"),
                        ("fromString", "from_string"),
                        ("toString", "to_string"),
                        ("from", "from_id"),
                        ("to", "to_id"),
                    ] {
                        if let Some(v) = item.get(source).filter(|v| !v.is_null()) {
                            out.insert(target.into(), v.clone());
                        }
                    }
                    Value::Object(out)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut out = serde_json::Map::new();
    if !items.is_empty() {
        out.insert("items".into(), Value::Array(items));
    }
    if let Some(author) = value.get("author") {
        out.insert("author".into(), simplify_user(author));
    }
    if let Some(created) = value_str(value, "created") {
        out.insert("created".into(), Value::String(created.to_string()));
    }
    Value::Object(out)
}

fn apply_projects_filter(jql: &str, filter: &str) -> String {
    let projects: Vec<&str> = filter
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if projects.is_empty() {
        return jql.to_string();
    }
    let lower = jql.to_ascii_lowercase();
    if lower.contains("project = ") || lower.contains("project in") {
        return jql.to_string();
    }
    let project_query = if projects.len() == 1 {
        format!("project = \"{}\"", escape_jql(projects[0]))
    } else {
        let list = projects
            .iter()
            .map(|p| format!("\"{}\"", escape_jql(p)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("project IN ({list})")
    };
    if jql.trim().is_empty() {
        return project_query;
    }
    if let Some((body, order)) = split_order_by(jql) {
        return format!("({body}) AND {project_query} {order}");
    }
    format!("({jql}) AND {project_query}")
}

fn split_order_by(jql: &str) -> Option<(&str, &str)> {
    let upper = jql.to_ascii_uppercase();
    let index = upper.rfind("ORDER BY")?;
    let body = jql[..index].trim_end();
    let order = jql[index..].trim();
    Some((body, order))
}

pub fn escape_jql(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::config::AtlassianConfig;
    use crate::providers::jira::JiraClient;

    fn client(server: &MockServer) -> JiraClient {
        let config = AtlassianConfig {
            base_url: server.uri(),
            username: Some("user".to_string()),
            token: "token".to_string(),
        };
        JiraClient::new(&config, reqwest::Client::new(), 0, Vec::new())
    }

    #[tokio::test]
    async fn search_uses_jql_endpoint_and_next_page_token() {
        let server = MockServer::start().await;
        let auth = "Basic dXNlcjp0b2tlbg==";

        Mock::given(method("POST"))
            .and(path("/rest/api/3/search/jql"))
            .and(header("authorization", auth))
            .and(body_json(json!({
                "jql": "project = TEST",
                "fields": ["summary"],
                "maxResults": 2
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issues": [{"id": "1", "key": "TEST-1", "fields": {"summary": "one"}}],
                "nextPageToken": "token2"
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/rest/api/3/search/jql"))
            .and(header("authorization", auth))
            .and(body_json(json!({
                "jql": "project = TEST",
                "fields": ["summary"],
                "maxResults": 1,
                "nextPageToken": "token2"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issues": [{"id": "2", "key": "TEST-2", "fields": {"summary": "two"}}]
            })))
            .mount(&server)
            .await;

        let result = client(&server)
            .search("project = TEST", Some("summary"), 2, 0, None, None, None)
            .await
            .unwrap();
        let issues = result["issues"].as_array().unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0]["key"], "TEST-1");
        assert_eq!(issues[1]["key"], "TEST-2");
    }
}
