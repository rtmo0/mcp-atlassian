use serde_json::{Value, json};

use crate::model::{Attachment, AttachmentBatch, AttachmentPayload, is_image_mime};
use crate::providers::jira::JiraClient;

impl JiraClient {
    pub async fn get_issue_attachments(&self, issue_key: &str) -> anyhow::Result<Vec<Attachment>> {
        self.ensure_issue_allowed(issue_key)?;
        let issue = self
            .client
            .get_json(
                &format!("rest/api/3/issue/{issue_key}"),
                &[("fields", "attachment".to_string())],
            )
            .await?;
        Ok(issue
            .get("fields")
            .and_then(|fields| fields.get("attachment"))
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Attachment::from_jira).collect())
            .unwrap_or_default())
    }

    pub async fn fetch_issue_attachments(
        &self,
        issue_key: &str,
        only_images: bool,
        max_file_bytes: usize,
        max_count: usize,
        max_total_bytes: usize,
    ) -> anyhow::Result<AttachmentBatch> {
        let attachments = self.get_issue_attachments(issue_key).await?;
        let total = attachments.len();
        let mut payloads = Vec::new();
        let mut failed = Vec::new();
        let mut total_bytes = 0usize;

        for attachment in attachments {
            if only_images {
                let guess = mime_guess::from_path(&attachment.filename)
                    .first_or_octet_stream()
                    .essence_str()
                    .to_string();
                if !is_image_mime(&attachment.mime_type) && !is_image_mime(&guess) {
                    continue;
                }
            }
            if payloads.len() >= max_count {
                failed.push(json!({
                    "filename": attachment.filename,
                    "error": format!("attachment limit reached ({max_count})"),
                }));
                continue;
            }
            if attachment
                .size
                .is_some_and(|size| size as usize > max_file_bytes)
            {
                failed.push(json!({
                    "filename": attachment.filename,
                    "error": format!("file is larger than {max_file_bytes} bytes"),
                }));
                continue;
            }
            let Some(url) = attachment.download_url.as_deref() else {
                failed.push(json!({
                    "filename": attachment.filename,
                    "error": "no download URL",
                }));
                continue;
            };
            match self.client.get_bytes_limited(url, max_file_bytes).await {
                Ok(bytes) => {
                    if bytes.len() > max_file_bytes {
                        failed.push(json!({
                            "filename": attachment.filename,
                            "error": format!("downloaded size {} exceeds limit", bytes.len()),
                        }));
                        continue;
                    }
                    if total_bytes + bytes.len() > max_total_bytes {
                        failed.push(json!({
                            "filename": attachment.filename,
                            "error": format!("total attachment limit {max_total_bytes} bytes exceeded"),
                        }));
                        continue;
                    }
                    total_bytes += bytes.len();
                    payloads.push(AttachmentPayload { attachment, bytes });
                }
                Err(error) => {
                    failed.push(json!({
                        "filename": attachment.filename,
                        "error": format!("{error:#}"),
                    }));
                }
            }
        }

        Ok(AttachmentBatch {
            payloads,
            failed,
            total,
            message: None,
        })
    }
}
