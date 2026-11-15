use chrono::{DateTime, Utc};
use serde_json::Value;

/// Convert an Atlassian Document Format (ADF) value to Markdown.
pub fn adf_to_markdown(value: &Value) -> String {
    let mut out = render(value, 0);
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
}

fn render(node: &Value, depth: usize) -> String {
    let node_type = node.get("type").and_then(Value::as_str).unwrap_or_default();
    let attrs = node.get("attrs").unwrap_or(&Value::Null);
    let content = node
        .get("content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    match node_type {
        "doc" => content
            .iter()
            .map(|child| render(child, depth))
            .collect::<Vec<_>>()
            .join("\n\n"),
        "paragraph" => render_inline_children(content),
        "heading" => {
            let level = attrs
                .get("level")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 6) as usize;
            format!("{} {}", "#".repeat(level), render_inline_children(content))
        }
        "text" => render_text(node),
        "hardBreak" => "\n".to_string(),
        "bulletList" => render_list(content, false, depth),
        "orderedList" => render_list(content, true, depth),
        "listItem" => render_children(content, depth),
        "codeBlock" => {
            let language = attrs
                .get("language")
                .and_then(Value::as_str)
                .unwrap_or_default();
            format!("```{language}\n{}\n```", text_content(node))
        }
        "blockquote" => prefix_lines(&render_children(content, depth), "> "),
        "panel" => prefix_lines(&render_children(content, depth), "> "),
        "rule" => "---".to_string(),
        "table" => render_table(content),
        "tableRow" | "tableCell" | "tableHeader" => render_children(content, depth),
        "mention" => {
            let name = attrs
                .get("text")
                .and_then(Value::as_str)
                .or_else(|| attrs.get("displayName").and_then(Value::as_str))
                .unwrap_or("user");
            format!("@{name}")
        }
        "emoji" => attrs
            .get("text")
            .and_then(Value::as_str)
            .or_else(|| attrs.get("shortName").and_then(Value::as_str))
            .unwrap_or("")
            .to_string(),
        "status" => format!(
            "`{}`",
            attrs
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("status")
        ),
        "date" => {
            if let Some(timestamp) = attrs.get("timestamp").and_then(Value::as_i64) {
                DateTime::<Utc>::from_timestamp_millis(timestamp)
                    .map(|dt| dt.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| timestamp.to_string())
            } else {
                String::new()
            }
        }
        "inlineCard" | "embedCard" => attrs
            .get("url")
            .and_then(Value::as_str)
            .map(|url| format!("<{url}>"))
            .unwrap_or_default(),
        "mediaSingle" | "mediaGroup" | "media" => {
            if node_type == "media" {
                let name = attrs
                    .get("alt")
                    .and_then(Value::as_str)
                    .or_else(|| attrs.get("id").and_then(Value::as_str))
                    .unwrap_or("media");
                format!("[media: {name}]")
            } else {
                render_children(content, depth)
            }
        }
        "expand" => {
            let title = attrs
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Details");
            format!(
                "<details><summary>{title}</summary>\n\n{}\n\n</details>",
                render_children(content, depth)
            )
        }
        "taskList" => content
            .iter()
            .map(|item| render_task_item(item, depth))
            .collect::<Vec<_>>()
            .join("\n"),
        "taskItem" => render_task_item(node, depth),
        "decisionList" => content
            .iter()
            .map(|item| render_decision_item(item, depth))
            .collect::<Vec<_>>()
            .join("\n"),
        "decisionItem" => render_decision_item(node, depth),
        "caption" => format!("_{}_", render_inline_children(content)),
        _ => render_children(content, depth),
    }
}

fn render_children(content: &[Value], depth: usize) -> String {
    content
        .iter()
        .map(|child| render(child, depth))
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_inline_children(content: &[Value]) -> String {
    content
        .iter()
        .map(|child| render(child, 0))
        .collect::<String>()
        .trim()
        .to_string()
}

fn render_text(node: &Value) -> String {
    let text = node.get("text").and_then(Value::as_str).unwrap_or_default();
    let mut out = text.to_string();
    if let Some(marks) = node.get("marks").and_then(Value::as_array) {
        for mark in marks {
            let mark_type = mark.get("type").and_then(Value::as_str).unwrap_or_default();
            let attrs = mark.get("attrs").unwrap_or(&Value::Null);
            out = match mark_type {
                "code" => format!("`{out}`"),
                "strong" => format!("**{out}**"),
                "em" => format!("*{out}*"),
                "strike" => format!("~~{out}~~"),
                "underline" => format!("<u>{out}</u>"),
                "subsup" => match attrs.get("type").and_then(Value::as_str) {
                    Some("sub") => format!("<sub>{out}</sub>"),
                    Some("sup") => format!("<sup>{out}</sup>"),
                    _ => out,
                },
                "link" => attrs
                    .get("href")
                    .and_then(Value::as_str)
                    .map(|href| format!("[{out}]({href})"))
                    .unwrap_or(out),
                _ => out,
            };
        }
    }
    out
}

fn render_list(items: &[Value], ordered: bool, depth: usize) -> String {
    let mut lines = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let body = render(item, depth);
        let marker = if ordered {
            format!("{}. ", index + 1)
        } else {
            "- ".to_string()
        };
        let indented = prefix_lines(&body, &" ".repeat(marker.len()));
        let first = indented
            .lines()
            .next()
            .unwrap_or_default()
            .strip_prefix(&" ".repeat(marker.len()))
            .unwrap_or_default();
        let rest: String = indented
            .lines()
            .skip(1)
            .map(|line| format!("\n{line}"))
            .collect();
        lines.push(format!("{marker}{first}{rest}"));
    }
    lines.join("\n")
}

fn render_task_item(node: &Value, depth: usize) -> String {
    let state = node
        .get("attrs")
        .and_then(|attrs| attrs.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("TODO");
    let checked = if state.eq_ignore_ascii_case("DONE") {
        "x"
    } else {
        " "
    };
    let body = render_children(
        node.get("content")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        depth,
    );
    let body = body.replace('\n', " ");
    format!("- [{checked}] {body}")
}

fn render_decision_item(node: &Value, depth: usize) -> String {
    let body = render_children(
        node.get("content")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        depth,
    );
    format!("- [decision] {}", body.replace('\n', " "))
}

fn render_table(rows: &[Value]) -> String {
    let parsed: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            row.get("content")
                .and_then(Value::as_array)
                .map(|cells| {
                    cells
                        .iter()
                        .map(|cell| {
                            render_inline_children(
                                cell.get("content")
                                    .and_then(Value::as_array)
                                    .map(Vec::as_slice)
                                    .unwrap_or(&[]),
                            )
                            .replace('|', "\\|")
                            .replace('\n', " ")
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
        .filter(|row: &Vec<String>| !row.is_empty())
        .collect();
    if parsed.is_empty() {
        return String::new();
    }
    let width = parsed.iter().map(Vec::len).max().unwrap_or(0);
    let mut out = String::new();
    for (index, row) in parsed.iter().enumerate() {
        let mut cells = row.clone();
        cells.resize(width, String::new());
        out.push('|');
        for cell in &cells {
            out.push_str(&format!(" {cell} |"));
        }
        out.push('\n');
        if index == 0 {
            out.push('|');
            for _ in 0..width {
                out.push_str(" --- |");
            }
            out.push('\n');
        }
    }
    out.trim_end().to_string()
}

fn prefix_lines(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_content(node: &Value) -> String {
    let mut out = String::new();
    if let Some(text) = node.get("text").and_then(Value::as_str) {
        out.push_str(text);
    }
    if let Some(content) = node.get("content").and_then(Value::as_array) {
        for child in content {
            out.push_str(&text_content(child));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_paragraph_with_marks() {
        let adf = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [
                    {"type": "text", "text": "Hello "},
                    {"type": "text", "text": "world", "marks": [{"type": "strong"}]}
                ]
            }]
        });
        assert_eq!(adf_to_markdown(&adf), "Hello **world**");
    }

    #[test]
    fn renders_lists_and_code() {
        let adf = json!({
            "type": "doc",
            "content": [
                {"type": "bulletList", "content": [
                    {"type": "listItem", "content": [
                        {"type": "paragraph", "content": [{"type": "text", "text": "one"}]}
                    ]},
                    {"type": "listItem", "content": [
                        {"type": "paragraph", "content": [{"type": "text", "text": "two"}]}
                    ]}
                ]},
                {"type": "codeBlock", "attrs": {"language": "sql"}, "content": [{"type": "text", "text": "select 1;"}]}
            ]
        });
        let md = adf_to_markdown(&adf);
        assert!(md.contains("- one"));
        assert!(md.contains("```sql\nselect 1;\n```"));
    }
}
