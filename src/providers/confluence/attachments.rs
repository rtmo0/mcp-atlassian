use serde_json::{Value, json};

use crate::model::{Attachment, AttachmentBatch, AttachmentPayload, is_image_mime};
use crate::providers::confluence::ConfluenceClient;

impl ConfluenceClient {
    pub async fn get_attachments(
        &self,
        content_id: &str,
        start: u32,
        limit: u32,
        filename: Option<&str>,
        media_type: Option<&str>,
    ) -> anyhow::Result<Value> {
        let limit = limit.clamp(1, 100) as usize;
        let all = self.list_all_attachments(content_id, 1000).await?;
        let filtered: Vec<Value> = all
            .into_iter()
            .filter(|attachment| {
                let name_matches = filename.is_none_or(|filename| {
                    attachment.get("title").and_then(Value::as_str) == Some(filename)
                });
                let type_matches = media_type.is_none_or(|media_type| {
                    attachment
                        .get("extensions")
                        .and_then(|extensions| extensions.get("mediaType"))
                        .and_then(Value::as_str)
                        == Some(media_type)
                });
                name_matches && type_matches
            })
            .collect();
        let total = filtered.len();
        let attachments: Vec<Value> = filtered
            .into_iter()
            .skip(start as usize)
            .take(limit)
            .collect();
        let simplified = attachments
            .iter()
            .map(|attachment| {
                crate::providers::confluence::simplify_attachment(attachment, self.base_url())
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "success": true,
            "content_id": content_id,
            "attachments": simplified,
            "total": total,
            "start": start,
            "limit": limit,
        }))
    }

    pub async fn download_attachment(
        &self,
        attachment_id: &str,
        max_file_bytes: usize,
        max_total_bytes: usize,
    ) -> anyhow::Result<AttachmentBatch> {
        let metadata = self
            .client
            .get_json(
                &format!("rest/api/content/{attachment_id}"),
                &[("expand", "extensions,version".to_string())],
            )
            .await?;
        let attachment = Attachment::from_confluence(&metadata, self.base_url())
            .ok_or_else(|| anyhow::anyhow!("failed to parse attachment {attachment_id}"))?;
        let mut failed = Vec::new();
        if attachment
            .size
            .is_some_and(|size| size as usize > max_file_bytes)
        {
            failed.push(json!({
                "filename": attachment.filename,
                "error": format!("file is larger than {max_file_bytes} bytes"),
            }));
            return Ok(AttachmentBatch {
                payloads: Vec::new(),
                failed,
                total: 1,
                message: None,
            });
        }
        let Some(url) = attachment.download_url.as_deref() else {
            failed.push(json!({
                "filename": attachment.filename,
                "error": "no download URL",
            }));
            return Ok(AttachmentBatch {
                payloads: Vec::new(),
                failed,
                total: 1,
                message: None,
            });
        };
        let bytes = self.client.get_bytes_limited(url, max_file_bytes).await?;
        if bytes.len() > max_file_bytes || bytes.len() > max_total_bytes {
            failed.push(json!({
                "filename": attachment.filename,
                "error": format!("downloaded size {} exceeds limit", bytes.len()),
            }));
            return Ok(AttachmentBatch {
                payloads: Vec::new(),
                failed,
                total: 1,
                message: None,
            });
        }
        Ok(AttachmentBatch {
            payloads: vec![AttachmentPayload { attachment, bytes }],
            failed,
            total: 1,
            message: None,
        })
    }

    pub async fn download_content_attachments(
        &self,
        content_id: &str,
        max_file_bytes: usize,
        max_count: usize,
        max_total_bytes: usize,
    ) -> anyhow::Result<AttachmentBatch> {
        self.fetch_attachments(
            content_id,
            false,
            max_file_bytes,
            max_count,
            max_total_bytes,
        )
        .await
    }

    pub async fn get_page_images(
        &self,
        content_id: &str,
        max_file_bytes: usize,
        max_count: usize,
        max_total_bytes: usize,
    ) -> anyhow::Result<AttachmentBatch> {
        self.fetch_attachments(content_id, true, max_file_bytes, max_count, max_total_bytes)
            .await
    }

    async fn fetch_attachments(
        &self,
        content_id: &str,
        only_images: bool,
        max_file_bytes: usize,
        max_count: usize,
        max_total_bytes: usize,
    ) -> anyhow::Result<AttachmentBatch> {
        let attachments = self.list_all_attachments(content_id, 1000).await?;
        let total = attachments.len();
        let mut payloads = Vec::new();
        let mut failed = Vec::new();
        let mut total_bytes = 0usize;
        let mut message = None;
        for value in attachments {
            let Some(attachment) = Attachment::from_confluence(&value, self.base_url()) else {
                continue;
            };
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
                message = Some(format!(
                    "attachment count limit {max_count} reached; remaining files were skipped"
                ));
                break;
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
            message,
        })
    }

    async fn list_all_attachments(
        &self,
        content_id: &str,
        max_items: usize,
    ) -> anyhow::Result<Vec<Value>> {
        let mut all = Vec::new();
        let mut start = 0u32;
        while all.len() < max_items {
            let page_size = 100u32.min((max_items - all.len()) as u32).max(1);
            let response = self
                .client
                .get_json(
                    &format!("rest/api/content/{content_id}/child/attachment"),
                    &[
                        ("start", start.to_string()),
                        ("limit", page_size.to_string()),
                        ("expand", "extensions,version".to_string()),
                    ],
                )
                .await?;
            let batch = response
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if batch.is_empty() {
                break;
            }
            let count = batch.len();
            all.extend(batch);
            start += count as u32;
            if response
                .get("_links")
                .and_then(|links| links.get("next"))
                .is_none()
            {
                break;
            }
        }
        all.truncate(max_items);
        Ok(all)
    }
}
