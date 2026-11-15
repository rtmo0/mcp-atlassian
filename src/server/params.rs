use schemars::JsonSchema;
use serde::Deserialize;

use crate::providers::jira::DEFAULT_READ_FIELDS;

pub(crate) fn default_10() -> u32 {
    10
}
pub(crate) fn default_25() -> u32 {
    25
}
pub(crate) fn default_50() -> u32 {
    50
}
pub(crate) fn default_100() -> u32 {
    100
}
pub(crate) fn default_true() -> bool {
    true
}
pub(crate) fn default_fields() -> String {
    DEFAULT_READ_FIELDS.to_string()
}
pub(crate) fn default_version_expand() -> String {
    "version".to_string()
}
pub(crate) fn default_group() -> String {
    "confluence-users".to_string()
}
pub(crate) fn default_minus_one() -> i64 {
    -1
}

// ---------------------------------------------------------------------------
// Jira parameter structs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetUserProfileParams {
    /// Identifier for the user (email, username, account ID, Server/DC key, or currentUser()).
    pub user_identifier: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraIssueKeyParams {
    /// Jira issue key (e.g. 'PROJ-123').
    pub issue_key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetIssueParams {
    /// Jira issue key (e.g. 'PROJ-123').
    pub issue_key: String,
    /// Comma-separated list of fields, or '*all'.
    #[serde(default = "default_fields")]
    #[schemars(default = "default_fields")]
    pub fields: String,
    /// Optional fields to expand (e.g. 'renderedFields,transitions,changelog').
    #[serde(default)]
    pub expand: Option<String>,
    /// Maximum number of comments to include (0 disables comments).
    #[serde(default = "default_10")]
    #[schemars(default = "default_10", range(min = 0, max = 100))]
    pub comment_limit: u32,
    /// Optional comma-separated issue properties to return.
    #[serde(default)]
    pub properties: Option<String>,
    /// Compatibility parameter. The server is read-only and does not update view history.
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    #[allow(dead_code)]
    pub update_history: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraSearchParams {
    /// JQL query string.
    pub jql: String,
    /// Comma-separated list of fields, or '*all'.
    #[serde(default = "default_fields")]
    #[schemars(default = "default_fields")]
    pub fields: String,
    /// Maximum number of results (1-50).
    #[serde(default = "default_10")]
    #[schemars(default = "default_10", range(min = 1, max = 50))]
    pub limit: u32,
    /// Starting index for pagination (0-based).
    #[serde(default)]
    pub start_at: u32,
    /// Optional comma-separated project keys filter.
    #[serde(default)]
    pub projects_filter: Option<String>,
    /// Optional fields to expand.
    #[serde(default)]
    pub expand: Option<String>,
    /// Pagination token from a previous search result (Cloud).
    #[serde(default)]
    pub page_token: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraSearchFieldsParams {
    /// Keyword for fuzzy search; empty lists the first N fields.
    #[serde(default)]
    pub keyword: String,
    /// Maximum number of results.
    #[serde(default = "default_10")]
    #[schemars(default = "default_10", range(min = 1))]
    pub limit: u32,
    /// Force refresh of the field list (the server does not cache).
    #[serde(default)]
    #[allow(dead_code)]
    pub refresh: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetFieldOptionsParams {
    /// Custom field ID (e.g. 'customfield_10001').
    pub field_id: String,
    /// Field context ID (Cloud). Auto-resolves to the global context.
    #[serde(default)]
    pub context_id: Option<String>,
    /// Project key (Server/DC only).
    #[serde(default)]
    #[allow(dead_code)]
    pub project_key: Option<String>,
    /// Issue type name (Server/DC only).
    #[serde(default)]
    #[allow(dead_code)]
    pub issue_type: Option<String>,
    /// Case-insensitive substring filter on option values.
    #[serde(default)]
    pub contains: Option<String>,
    /// Maximum number of options after filtering.
    #[serde(default)]
    #[schemars(range(min = 1))]
    pub return_limit: Option<u32>,
    /// Return only value strings in a compact format.
    #[serde(default)]
    pub values_only: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetProjectIssuesParams {
    /// Jira project key (e.g. 'PROJ').
    pub project_key: String,
    /// Maximum number of results (1-50).
    #[serde(default = "default_10")]
    #[schemars(default = "default_10", range(min = 1, max = 50))]
    pub limit: u32,
    /// Starting index for pagination (0-based).
    #[serde(default)]
    pub start_at: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetAgileBoardsParams {
    /// Board name (fuzzy search).
    #[serde(default)]
    pub board_name: Option<String>,
    /// Jira project key.
    #[serde(default)]
    pub project_key: Option<String>,
    /// Board type ('scrum' or 'kanban').
    #[serde(default)]
    pub board_type: Option<String>,
    /// Starting index for pagination (0-based).
    #[serde(default)]
    pub start_at: u32,
    /// Maximum number of results (1-50).
    #[serde(default = "default_10")]
    #[schemars(default = "default_10", range(min = 1, max = 50))]
    pub limit: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetBoardIssuesParams {
    /// Board ID (e.g. '1001').
    pub board_id: String,
    /// JQL query string.
    pub jql: String,
    /// Comma-separated list of fields, or '*all'.
    #[serde(default = "default_fields")]
    #[schemars(default = "default_fields")]
    pub fields: String,
    /// Starting index for pagination (0-based).
    #[serde(default)]
    pub start_at: u32,
    /// Maximum number of results (1-50).
    #[serde(default = "default_10")]
    #[schemars(default = "default_10", range(min = 1, max = 50))]
    pub limit: u32,
    /// Optional fields to expand.
    #[serde(default = "default_version_expand")]
    #[schemars(default = "default_version_expand")]
    pub expand: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetSprintsFromBoardParams {
    /// Board ID (e.g. '1000').
    pub board_id: String,
    /// Sprint state ('active', 'future', 'closed').
    #[serde(default)]
    pub state: Option<String>,
    /// Starting index for pagination (0-based).
    #[serde(default)]
    pub start_at: u32,
    /// Maximum number of results (1-50).
    #[serde(default = "default_10")]
    #[schemars(default = "default_10", range(min = 1, max = 50))]
    pub limit: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetSprintIssuesParams {
    /// Sprint ID (e.g. '10001').
    pub sprint_id: String,
    /// Comma-separated list of fields, or '*all'.
    #[serde(default = "default_fields")]
    #[schemars(default = "default_fields")]
    pub fields: String,
    /// Starting index for pagination (0-based).
    #[serde(default)]
    pub start_at: u32,
    /// Maximum number of results (1-50).
    #[serde(default = "default_10")]
    #[schemars(default = "default_10", range(min = 1, max = 50))]
    pub limit: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetLinkTypesParams {
    /// Optional substring filter on link type names.
    #[serde(default)]
    pub name_filter: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraBatchGetChangelogsParams {
    /// Comma-separated issue IDs or keys.
    pub issue_ids_or_keys: String,
    /// Optional comma-separated fields to filter changelogs by.
    #[serde(default)]
    pub fields: Option<String>,
    /// Maximum changelogs per issue (-1 for all).
    #[serde(default = "default_minus_one")]
    #[schemars(default = "default_minus_one")]
    pub limit: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraProjectKeyParams {
    /// Jira project key (e.g. 'PROJ').
    pub project_key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetAllProjectsParams {
    /// Whether to include archived projects.
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetServiceDeskQueuesParams {
    /// Service desk ID (e.g. '4').
    pub service_desk_id: String,
    /// Starting index for pagination (0-based).
    #[serde(default)]
    pub start_at: u32,
    /// Maximum number of results (1-50).
    #[serde(default = "default_50")]
    #[schemars(default = "default_50", range(min = 1, max = 50))]
    pub limit: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetQueueIssuesParams {
    /// Service desk ID (e.g. '4').
    pub service_desk_id: String,
    /// Queue ID (e.g. '47').
    pub queue_id: String,
    /// Starting index for pagination (0-based).
    #[serde(default)]
    pub start_at: u32,
    /// Maximum number of results (1-50).
    #[serde(default = "default_50")]
    #[schemars(default = "default_50", range(min = 1, max = 50))]
    pub limit: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetProformaFormDetailsParams {
    /// Jira issue key.
    pub issue_key: String,
    /// ProForma form UUID.
    pub form_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetIssueDatesParams {
    /// Jira issue key.
    pub issue_key: String,
    /// Include status change history with timestamps and durations.
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    pub include_status_changes: bool,
    /// Include aggregated time spent in each status.
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    pub include_status_summary: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetIssueSlaParams {
    /// Jira issue key.
    pub issue_key: String,
    /// Comma-separated SLA metrics to calculate.
    #[serde(default)]
    pub metrics: Option<String>,
    /// Calculate using working hours only.
    #[serde(default)]
    pub working_hours_only: Option<bool>,
    /// Include raw date values in the response.
    #[serde(default)]
    pub include_raw_dates: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetIssueDevelopmentInfoParams {
    /// Jira issue key.
    pub issue_key: String,
    /// Optional application type filter (stash, bitbucket, github, gitlab).
    #[serde(default)]
    pub application_type: Option<String>,
    /// Optional data type filter (pullrequest, branch, repository).
    #[serde(default)]
    pub data_type: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JiraGetIssuesDevelopmentInfoParams {
    /// Comma-separated Jira issue keys.
    pub issue_keys: String,
    /// Optional application type filter.
    #[serde(default)]
    pub application_type: Option<String>,
    /// Optional data type filter.
    #[serde(default)]
    pub data_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Confluence parameter structs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceSearchParams {
    /// Simple search text or a CQL query.
    pub query: String,
    /// Maximum number of results (1-50).
    #[serde(default = "default_10")]
    #[schemars(default = "default_10", range(min = 1, max = 50))]
    pub limit: u32,
    /// Optional comma-separated space keys filter.
    #[serde(default)]
    pub spaces_filter: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceGetPageParams {
    /// Confluence page ID.
    #[serde(default)]
    pub page_id: Option<String>,
    /// Exact page title (use with space_key).
    #[serde(default)]
    pub title: Option<String>,
    /// Space key (required with title).
    #[serde(default)]
    pub space_key: Option<String>,
    /// Include page metadata (dates, version, labels, attachments).
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    pub include_metadata: bool,
    /// Convert page storage HTML to Markdown.
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    pub convert_to_markdown: bool,
    /// 0-based character offset for paginated content; omitted means 0.
    #[serde(default)]
    #[schemars(range(min = 0))]
    pub content_offset: Option<u32>,
    /// Maximum characters per chunk; omitted uses the server default
    /// (1000..=ATLASSIAN_MCP_MAX_CONTENT_CHARS).
    #[serde(default)]
    #[schemars(range(min = 1000))]
    pub content_limit: Option<u32>,
    /// Expected page version; required when content_offset > 0.
    #[serde(default)]
    #[schemars(range(min = 1))]
    pub expected_version: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceGetPageChildrenParams {
    /// Parent page ID.
    pub parent_id: String,
    /// Fields to expand (default 'version').
    #[serde(default = "default_version_expand")]
    #[schemars(default = "default_version_expand")]
    pub expand: String,
    /// Maximum number of child items (1-50).
    #[serde(default = "default_25")]
    #[schemars(default = "default_25", range(min = 1, max = 50))]
    pub limit: u32,
    /// Include page content in the response.
    #[serde(default)]
    pub include_content: bool,
    /// Convert content to Markdown.
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    pub convert_to_markdown: bool,
    /// Starting index for pagination (0-based).
    #[serde(default)]
    pub start: u32,
    /// Include child folders in addition to pages.
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    pub include_folders: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceGetSpacePageTreeParams {
    /// Space key.
    pub space_key: String,
    /// Maximum pages to fetch (1-1000).
    #[serde(default = "default_100")]
    #[schemars(default = "default_100", range(min = 1, max = 1000))]
    pub limit: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluencePageIdParams {
    /// Confluence page ID.
    pub page_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceSearchUserParams {
    /// CQL query string for user search.
    pub query: String,
    /// Maximum number of results (1-50).
    #[serde(default = "default_10")]
    #[schemars(default = "default_10", range(min = 1, max = 50))]
    pub limit: u32,
    /// Group to search within on Server/DC (ignored on Cloud).
    #[serde(default = "default_group")]
    #[schemars(default = "default_group")]
    #[allow(dead_code)]
    pub group_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceGetPageHistoryParams {
    /// Confluence page ID.
    pub page_id: String,
    /// Version number (>= 1).
    #[schemars(range(min = 1))]
    pub version: u32,
    /// Convert content to Markdown.
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    pub convert_to_markdown: bool,
    /// 0-based character offset for paginated content; omitted means 0.
    #[serde(default)]
    #[schemars(range(min = 0))]
    pub content_offset: Option<u32>,
    /// Maximum characters per chunk; omitted uses the server default
    /// (1000..=ATLASSIAN_MCP_MAX_CONTENT_CHARS).
    #[serde(default)]
    #[schemars(range(min = 1000))]
    pub content_limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceGetPageDiffParams {
    /// Confluence page ID.
    pub page_id: String,
    /// Source version number.
    #[schemars(range(min = 1))]
    pub from_version: u32,
    /// Target version number.
    #[schemars(range(min = 1))]
    pub to_version: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceGetPageViewsParams {
    /// Confluence page ID.
    pub page_id: String,
    /// Include the page title in the response.
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    pub include_title: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceGetAttachmentsParams {
    /// Confluence content ID (page or blog post).
    pub content_id: String,
    /// Starting index for pagination (0-based).
    #[serde(default)]
    pub start: u32,
    /// Maximum number of attachments (1-100).
    #[serde(default = "default_50")]
    #[schemars(default = "default_50", range(min = 1, max = 100))]
    pub limit: u32,
    /// Exact filename filter.
    #[serde(default)]
    pub filename: Option<String>,
    /// MIME type filter.
    #[serde(default)]
    pub media_type: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceAttachmentIdParams {
    /// Confluence attachment ID (e.g. 'att123456789').
    pub attachment_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceContentIdParams {
    /// Confluence content ID (page or blog post).
    pub content_id: String,
}
