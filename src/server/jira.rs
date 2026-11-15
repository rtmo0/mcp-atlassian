use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{ErrorData as McpError, tool, tool_router};

use crate::server::AtlassianMcpServer;
use crate::server::params::*;
use crate::server::render::{attachment_result, attachment_summary};

#[tool_router(router = jira_tools, vis = "pub(crate)")]
impl AtlassianMcpServer {
    #[tool(
        description = "Retrieve profile information for a specific Jira user. Use currentUser() for the authenticated user.",
        annotations(title = "Get User Profile", read_only_hint = true)
    )]
    async fn jira_get_user_profile(
        &self,
        Parameters(params): Parameters<JiraGetUserProfileParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_user_profile", |jira| async move {
            jira.get_user_profile(&params.user_identifier).await
        })
        .await
    }

    #[tool(
        description = "Get the list of watchers for a Jira issue.",
        annotations(title = "Get Issue Watchers", read_only_hint = true)
    )]
    async fn jira_get_issue_watchers(
        &self,
        Parameters(params): Parameters<JiraIssueKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_issue_watchers", |jira| async move {
            jira.get_issue_watchers(&params.issue_key).await
        })
        .await
    }

    #[tool(
        description = "Get details of a specific Jira issue including its Epic links and relationship information.",
        annotations(title = "Get Issue", read_only_hint = true)
    )]
    async fn jira_get_issue(
        &self,
        Parameters(params): Parameters<JiraGetIssueParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_issue", |jira| async move {
            jira.get_issue(
                &params.issue_key,
                Some(&params.fields),
                params.expand.as_deref(),
                params.comment_limit,
                params.properties.as_deref(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Search Jira issues using JQL (Jira Query Language).",
        annotations(title = "Search Issues", read_only_hint = true)
    )]
    async fn jira_search(
        &self,
        Parameters(params): Parameters<JiraSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_search", |jira| async move {
            jira.search(
                &params.jql,
                Some(&params.fields),
                params.limit,
                params.start_at,
                params.expand.as_deref(),
                params.projects_filter.as_deref(),
                params.page_token.as_deref(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Search Jira fields by keyword with fuzzy match.",
        annotations(title = "Search Fields", read_only_hint = true)
    )]
    async fn jira_search_fields(
        &self,
        Parameters(params): Parameters<JiraSearchFieldsParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_search_fields", |jira| async move {
            jira.search_fields(&params.keyword, params.limit as usize, params.refresh)
                .await
        })
        .await
    }

    #[tool(
        description = "Get allowed option values for a Jira custom field.",
        annotations(title = "Get Field Options", read_only_hint = true)
    )]
    async fn jira_get_field_options(
        &self,
        Parameters(params): Parameters<JiraGetFieldOptionsParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_field_options", |jira| async move {
            jira.get_field_options(
                &params.field_id,
                params.context_id.as_deref(),
                params.contains.as_deref(),
                params.return_limit.map(|value| value as usize),
                params.values_only,
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get all issues for a specific Jira project.",
        annotations(title = "Get Project Issues", read_only_hint = true)
    )]
    async fn jira_get_project_issues(
        &self,
        Parameters(params): Parameters<JiraGetProjectIssuesParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_project_issues", |jira| async move {
            jira.get_project_issues(&params.project_key, params.limit, params.start_at)
                .await
        })
        .await
    }

    #[tool(
        description = "Get available status transitions for a Jira issue.",
        annotations(title = "Get Transitions", read_only_hint = true)
    )]
    async fn jira_get_transitions(
        &self,
        Parameters(params): Parameters<JiraIssueKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_transitions", |jira| async move {
            jira.get_transitions(&params.issue_key).await
        })
        .await
    }

    #[tool(
        description = "Get worklog entries for a Jira issue.",
        annotations(title = "Get Worklog", read_only_hint = true)
    )]
    async fn jira_get_worklog(
        &self,
        Parameters(params): Parameters<JiraIssueKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_worklog", |jira| async move {
            jira.get_worklogs(&params.issue_key).await
        })
        .await
    }

    #[tool(
        description = "Download attachments from a Jira issue as embedded MCP resources.",
        annotations(title = "Download Attachments", read_only_hint = true)
    )]
    async fn jira_download_attachments(
        &self,
        Parameters(params): Parameters<JiraIssueKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_tool("jira_download_attachments", |jira| async move {
            let batch = jira
                .fetch_issue_attachments(
                    &params.issue_key,
                    false,
                    self.providers.max_attachment_bytes,
                    self.providers.max_attachments_per_call,
                    self.providers.max_total_attachment_bytes,
                )
                .await?;
            let summary = attachment_summary(
                &batch,
                serde_json::json!({
                    "success": true,
                    "issue_key": params.issue_key,
                }),
                "downloaded",
            );
            Ok(attachment_result(batch, summary, false))
        })
        .await
    }

    #[tool(
        description = "Get all images attached to a Jira issue as inline MCP image content.",
        annotations(title = "Get Issue Images", read_only_hint = true)
    )]
    async fn jira_get_issue_images(
        &self,
        Parameters(params): Parameters<JiraIssueKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_tool("jira_get_issue_images", |jira| async move {
            let batch = jira
                .fetch_issue_attachments(
                    &params.issue_key,
                    true,
                    self.providers.max_attachment_bytes,
                    self.providers.max_attachments_per_call,
                    self.providers.max_total_attachment_bytes,
                )
                .await?;
            let summary = attachment_summary(
                &batch,
                serde_json::json!({
                    "success": true,
                    "issue_key": params.issue_key,
                }),
                "downloaded",
            );
            Ok(attachment_result(batch, summary, true))
        })
        .await
    }

    #[tool(
        description = "Get Jira agile boards by name, project key, or type.",
        annotations(title = "Get Agile Boards", read_only_hint = true)
    )]
    async fn jira_get_agile_boards(
        &self,
        Parameters(params): Parameters<JiraGetAgileBoardsParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_agile_boards", |jira| async move {
            jira.get_agile_boards(
                params.board_name.as_deref(),
                params.project_key.as_deref(),
                params.board_type.as_deref(),
                params.start_at,
                params.limit,
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get all issues linked to a specific board filtered by JQL.",
        annotations(title = "Get Board Issues", read_only_hint = true)
    )]
    async fn jira_get_board_issues(
        &self,
        Parameters(params): Parameters<JiraGetBoardIssuesParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_board_issues", |jira| async move {
            jira.get_board_issues(
                &params.board_id,
                &params.jql,
                Some(&params.fields),
                params.start_at,
                params.limit,
                Some(&params.expand),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get Jira sprints from a board by state.",
        annotations(title = "Get Sprints from Board", read_only_hint = true)
    )]
    async fn jira_get_sprints_from_board(
        &self,
        Parameters(params): Parameters<JiraGetSprintsFromBoardParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_sprints_from_board", |jira| async move {
            jira.get_sprints_from_board(
                &params.board_id,
                params.state.as_deref(),
                params.start_at,
                params.limit,
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get Jira issues from a sprint.",
        annotations(title = "Get Sprint Issues", read_only_hint = true)
    )]
    async fn jira_get_sprint_issues(
        &self,
        Parameters(params): Parameters<JiraGetSprintIssuesParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_sprint_issues", |jira| async move {
            jira.get_sprint_issues(
                &params.sprint_id,
                Some(&params.fields),
                params.start_at,
                params.limit,
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get all available Jira issue link types.",
        annotations(title = "Get Link Types", read_only_hint = true)
    )]
    async fn jira_get_link_types(
        &self,
        Parameters(params): Parameters<JiraGetLinkTypesParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_link_types", |jira| async move {
            jira.get_link_types(params.name_filter.as_deref()).await
        })
        .await
    }

    #[tool(
        description = "Get changelogs for multiple Jira issues (Cloud only).",
        annotations(title = "Batch Get Changelogs", read_only_hint = true)
    )]
    async fn jira_batch_get_changelogs(
        &self,
        Parameters(params): Parameters<JiraBatchGetChangelogsParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_batch_get_changelogs", |jira| async move {
            let keys: Vec<String> = params
                .issue_ids_or_keys
                .split(',')
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            let fields: Option<Vec<String>> = params.fields.as_deref().map(|fields| {
                fields
                    .split(',')
                    .map(str::trim)
                    .filter(|field| !field.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            });
            jira.batch_get_changelogs(&keys, fields.as_deref(), params.limit)
                .await
        })
        .await
    }

    #[tool(
        description = "Get all fix versions for a specific Jira project.",
        annotations(title = "Get Project Versions", read_only_hint = true)
    )]
    async fn jira_get_project_versions(
        &self,
        Parameters(params): Parameters<JiraProjectKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_project_versions", |jira| async move {
            jira.get_project_versions(&params.project_key).await
        })
        .await
    }

    #[tool(
        description = "Get all components for a specific Jira project.",
        annotations(title = "Get Project Components", read_only_hint = true)
    )]
    async fn jira_get_project_components(
        &self,
        Parameters(params): Parameters<JiraProjectKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_project_components", |jira| async move {
            jira.get_project_components(&params.project_key).await
        })
        .await
    }

    #[tool(
        description = "Get all Jira projects accessible to the current user.",
        annotations(title = "Get All Projects", read_only_hint = true)
    )]
    async fn jira_get_all_projects(
        &self,
        Parameters(params): Parameters<JiraGetAllProjectsParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_all_projects", |jira| async move {
            jira.get_all_projects(params.include_archived).await
        })
        .await
    }

    #[tool(
        description = "Get the Jira Service Desk associated with a project key. Cloud is not supported by the upstream implementation.",
        annotations(title = "Get Service Desk For Project", read_only_hint = true)
    )]
    async fn jira_get_service_desk_for_project(
        &self,
        Parameters(params): Parameters<JiraProjectKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_service_desk_for_project", |jira| async move {
            jira.get_service_desk_for_project(&params.project_key).await
        })
        .await
    }

    #[tool(
        description = "Get queues for a Jira Service Desk. Cloud is not supported by the upstream implementation.",
        annotations(title = "Get Service Desk Queues", read_only_hint = true)
    )]
    async fn jira_get_service_desk_queues(
        &self,
        Parameters(params): Parameters<JiraGetServiceDeskQueuesParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_service_desk_queues", |jira| async move {
            jira.get_service_desk_queues(&params.service_desk_id, params.start_at, params.limit)
                .await
        })
        .await
    }

    #[tool(
        description = "Get issues from a Jira Service Desk queue. Cloud is not supported by the upstream implementation.",
        annotations(title = "Get Queue Issues", read_only_hint = true)
    )]
    async fn jira_get_queue_issues(
        &self,
        Parameters(params): Parameters<JiraGetQueueIssuesParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_queue_issues", |jira| async move {
            jira.get_queue_issues(
                &params.service_desk_id,
                &params.queue_id,
                params.start_at,
                params.limit,
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get all ProForma forms associated with a Jira issue.",
        annotations(title = "Get Issue Forms", read_only_hint = true)
    )]
    async fn jira_get_issue_proforma_forms(
        &self,
        Parameters(params): Parameters<JiraIssueKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_issue_proforma_forms", |jira| async move {
            jira.get_issue_proforma_forms(&params.issue_key).await
        })
        .await
    }

    #[tool(
        description = "Get detailed information about a specific ProForma form.",
        annotations(title = "Get Form Details", read_only_hint = true)
    )]
    async fn jira_get_proforma_form_details(
        &self,
        Parameters(params): Parameters<JiraGetProformaFormDetailsParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_proforma_form_details", |jira| async move {
            jira.get_proforma_form_details(&params.issue_key, &params.form_id)
                .await
        })
        .await
    }

    #[tool(
        description = "Get date information and status transition history for a Jira issue.",
        annotations(title = "Get Issue Dates", read_only_hint = true)
    )]
    async fn jira_get_issue_dates(
        &self,
        Parameters(params): Parameters<JiraGetIssueDatesParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_issue_dates", |jira| async move {
            jira.get_issue_dates(
                &params.issue_key,
                params.include_status_changes,
                params.include_status_summary,
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Calculate SLA metrics for a Jira issue.",
        annotations(title = "Get Issue SLA", read_only_hint = true)
    )]
    async fn jira_get_issue_sla(
        &self,
        Parameters(params): Parameters<JiraGetIssueSlaParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_issue_sla", |jira| async move {
            let metrics: Option<Vec<String>> = params.metrics.as_deref().map(|metrics| {
                metrics
                    .split(',')
                    .map(str::trim)
                    .filter(|metric| !metric.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            });
            jira.get_issue_sla(
                &params.issue_key,
                metrics.as_deref(),
                params.working_hours_only,
                params.include_raw_dates,
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get development information (PRs, commits, branches) linked to a Jira issue.",
        annotations(title = "Get Issue Development Info", read_only_hint = true)
    )]
    async fn jira_get_issue_development_info(
        &self,
        Parameters(params): Parameters<JiraGetIssueDevelopmentInfoParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_issue_development_info", |jira| async move {
            jira.get_issue_development_info(
                &params.issue_key,
                params.application_type.as_deref(),
                params.data_type.as_deref(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get development information for multiple Jira issues.",
        annotations(title = "Get Issues Development Info", read_only_hint = true)
    )]
    async fn jira_get_issues_development_info(
        &self,
        Parameters(params): Parameters<JiraGetIssuesDevelopmentInfoParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run_jira_json("jira_get_issues_development_info", |jira| async move {
            let keys: Vec<String> = params
                .issue_keys
                .split(',')
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            jira.get_issues_development_info(
                &keys,
                params.application_type.as_deref(),
                params.data_type.as_deref(),
            )
            .await
        })
        .await
    }
}
