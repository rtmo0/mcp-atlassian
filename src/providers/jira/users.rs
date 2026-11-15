use regex::Regex;
use serde_json::{Value, json};
use std::sync::OnceLock;

use crate::providers::jira::{JiraClient, simplify_user};

impl JiraClient {
    pub async fn get_user_profile(&self, identifier: &str) -> anyhow::Result<Value> {
        let user = if is_current_user(identifier) {
            self.client.get_json("rest/api/3/myself", &[]).await?
        } else {
            let account_id = if looks_like_account_id(identifier) {
                identifier.to_string()
            } else {
                self.resolve_account_id(identifier).await?
            };
            self.client
                .get_json("rest/api/3/user", &[("accountId", account_id)])
                .await?
        };
        Ok(json!({
            "success": true,
            "user": simplify_user(&user),
        }))
    }

    async fn resolve_account_id(&self, identifier: &str) -> anyhow::Result<String> {
        let response = self
            .client
            .get_json(
                "rest/api/3/user/search",
                &[
                    ("query", identifier.to_string()),
                    ("maxResults", "10".to_string()),
                ],
            )
            .await?;
        let users = response.as_array().cloned().unwrap_or_default();
        let exact = users.iter().find(|user| {
            user.get("emailAddress")
                .and_then(Value::as_str)
                .is_some_and(|email| email.eq_ignore_ascii_case(identifier))
                || user
                    .get("displayName")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.eq_ignore_ascii_case(identifier))
        });
        exact
            .or_else(|| users.first())
            .and_then(|user| user.get("accountId"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow::anyhow!("user '{identifier}' not found"))
    }
}

fn is_current_user(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "currentuser()" | "currentuser" | "current_user" | "me" | "myself"
    )
}

fn looks_like_account_id(value: &str) -> bool {
    static HEX: OnceLock<Regex> = OnceLock::new();
    static PREFIXED: OnceLock<Regex> = OnceLock::new();
    let hex = HEX.get_or_init(|| Regex::new(r"^[0-9a-fA-F]{24}$").expect("valid regex"));
    let prefixed = PREFIXED.get_or_init(|| Regex::new(r"^\d+:\w+").expect("valid regex"));
    hex.is_match(value) || prefixed.is_match(value)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::config::AtlassianConfig;
    use crate::providers::jira::JiraClient;

    #[tokio::test]
    async fn current_user_alias_uses_myself_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/myself"))
            .and(header("authorization", "Basic dXNlcjp0b2tlbg=="))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "accountId": "630dafec9794410874c58fa0",
                "displayName": "Me",
                "emailAddress": "me@example.com",
                "active": true
            })))
            .mount(&server)
            .await;

        let config = AtlassianConfig {
            base_url: server.uri(),
            username: Some("user".to_string()),
            token: "token".to_string(),
        };
        let client = JiraClient::new(&config, reqwest::Client::new(), 0, Vec::new());
        let result = client.get_user_profile("currentUser()").await.unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["user"]["display_name"], "Me");
        assert_eq!(result["user"]["email"], "me@example.com");
    }
}
