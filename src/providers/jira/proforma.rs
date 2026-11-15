use serde_json::{Value, json};

use crate::providers::jira::JiraClient;

const CLOUD_ID_ENV: &str = "ATLASSIAN_OAUTH_CLOUD_ID";

impl JiraClient {
    pub async fn get_issue_proforma_forms(&self, issue_key: &str) -> anyhow::Result<Value> {
        let Some(cloud_id) = cloud_id() else {
            return Ok(forms_error(
                "Forms API requires a cloud_id. For OAuth, this is automatically retrieved. For other auth types, provide it via ATLASSIAN_OAUTH_CLOUD_ID environment variable or X-Atlassian-Cloud-Id header.",
                Some(issue_key),
                None,
            ));
        };
        let url =
            format!("https://api.atlassian.com/jira/forms/cloud/{cloud_id}/issue/{issue_key}/form");
        match self.client.get_json_absolute(&url).await {
            Ok(response) => {
                let forms = if response.is_array() {
                    response.as_array().cloned().unwrap_or_default()
                } else {
                    response
                        .get("forms")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                };
                let count = forms.len();
                Ok(json!({
                    "success": true,
                    "forms": forms,
                    "count": count,
                }))
            }
            Err(error) => Ok(forms_error(&format!("{error:#}"), Some(issue_key), None)),
        }
    }

    pub async fn get_proforma_form_details(
        &self,
        issue_key: &str,
        form_id: &str,
    ) -> anyhow::Result<Value> {
        let Some(cloud_id) = cloud_id() else {
            return Ok(forms_error(
                "Forms API requires a cloud_id. For OAuth, this is automatically retrieved. For other auth types, provide it via ATLASSIAN_OAUTH_CLOUD_ID environment variable or X-Atlassian-Cloud-Id header.",
                Some(issue_key),
                Some(form_id),
            ));
        };
        let url = format!(
            "https://api.atlassian.com/jira/forms/cloud/{cloud_id}/issue/{issue_key}/form/{form_id}"
        );
        match self.client.get_json_absolute(&url).await {
            Ok(form) => Ok(json!({"success": true, "form": form})),
            Err(error) => {
                if let Some(api_error) = error.downcast_ref::<crate::model::ApiError>()
                    && api_error.status == Some(404)
                {
                    return Ok(json!({
                        "success": false,
                        "error": format!("Form {form_id} not found for issue {issue_key}"),
                        "issue_key": issue_key,
                        "form_id": form_id,
                    }));
                }
                Ok(forms_error(
                    &format!("{error:#}"),
                    Some(issue_key),
                    Some(form_id),
                ))
            }
        }
    }
}

fn cloud_id() -> Option<String> {
    std::env::var(CLOUD_ID_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn forms_error(error: &str, issue_key: Option<&str>, form_id: Option<&str>) -> Value {
    json!({
        "success": false,
        "error": error,
        "issue_key": issue_key,
        "form_id": form_id,
    })
}
