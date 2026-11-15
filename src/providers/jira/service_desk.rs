use serde_json::Value;

use crate::providers::jira::JiraClient;

impl JiraClient {
    /// Service Desk queue read endpoints in mcp-atlassian are Server/DC-only.
    /// On Cloud the upstream server raises this exact error, so mirror it.
    pub async fn get_service_desk_for_project(&self, _project_key: &str) -> anyhow::Result<Value> {
        anyhow::bail!(
            "Jira Service Desk queue read endpoints are currently implemented for Server/Data Center in v1."
        )
    }

    pub async fn get_service_desk_queues(
        &self,
        _service_desk_id: &str,
        _start_at: u32,
        _limit: u32,
    ) -> anyhow::Result<Value> {
        anyhow::bail!(
            "Jira Service Desk queue read endpoints are currently implemented for Server/Data Center in v1."
        )
    }

    pub async fn get_queue_issues(
        &self,
        _service_desk_id: &str,
        _queue_id: &str,
        _start_at: u32,
        _limit: u32,
    ) -> anyhow::Result<Value> {
        anyhow::bail!(
            "Jira Service Desk queue read endpoints are currently implemented for Server/Data Center in v1."
        )
    }
}
