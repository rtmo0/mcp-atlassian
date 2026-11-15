use std::sync::OnceLock;

use regex::{Captures, Regex};

/// Convert Confluence storage-format XHTML to Markdown.
pub fn storage_to_markdown(storage: &str) -> String {
    if storage.trim().is_empty() {
        return String::new();
    }
    let html = preprocess(storage);
    let mut markdown = htmd::convert(&html).unwrap_or(html);
    markdown = markdown.replace("\\[ \\]", "[ ]").replace("\\[x\\]", "[x]");
    while markdown.contains("\n\n\n") {
        markdown = markdown.replace("\n\n\n", "\n\n");
    }
    markdown.trim().to_string()
}

fn preprocess(storage: &str) -> String {
    let mut out = storage.to_string();

    out = code_macro_re()
        .replace_all(&out, |caps: &Captures<'_>| {
            let whole = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
            let code = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let language = language_re()
                .captures(whole)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string())
                .filter(|v| !v.is_empty());
            let escaped = html_escape(code);
            match language {
                Some(language) => {
                    format!("<pre><code class=\"language-{language}\">{escaped}</code></pre>")
                }
                None => format!("<pre><code>{escaped}</code></pre>"),
            }
        })
        .into_owned();

    out = panel_macro_re()
        .replace_all(&out, |caps: &Captures<'_>| {
            let body = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
            format!("<blockquote>{body}</blockquote>")
        })
        .into_owned();

    out = toc_macro_re().replace_all(&out, "").into_owned();
    out = jira_macro_re()
        .replace_all(&out, "<code>$1</code>")
        .into_owned();
    out = include_macro_re()
        .replace_all(&out, "<em>[include: $1]</em>")
        .into_owned();

    out = task_re()
        .replace_all(&out, |caps: &Captures<'_>| {
            let whole = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
            let done = task_status_re()
                .captures(whole)
                .and_then(|c| c.get(1))
                .is_some_and(|m| m.as_str().eq_ignore_ascii_case("complete"));
            let body = task_body_re()
                .captures(whole)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| {
                    caps.get(1)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default()
                });
            format!("<li>[{}] {body}</li>", if done { "x" } else { " " })
        })
        .into_owned();
    out = task_list_open_re().replace_all(&out, "<ul>").into_owned();
    out = task_list_close_re().replace_all(&out, "</ul>").into_owned();

    out = emoticon_re().replace_all(&out, ":$1:").into_owned();
    out = self_closing_time_re().replace_all(&out, "$1").into_owned();
    out = out.replace("@@@hl@@@", "").replace("@@@endhl@@@", "");

    out = parameter_re().replace_all(&out, "").into_owned();
    out = inline_comment_re().replace_all(&out, "").into_owned();

    out = link_re()
        .replace_all(&out, |caps: &Captures<'_>| {
            let whole = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
            if let Some(body) = link_body_re().captures(whole) {
                return body
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
            }
            if let Some(title) = content_title_re().captures(whole) {
                return title
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
            }
            strip_tags(whole)
        })
        .into_owned();

    out = image_re()
        .replace_all(&out, |caps: &Captures<'_>| {
            let whole = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
            if let Some(url) = ri_url_re().captures(whole) {
                return format!(
                    "![image]({})",
                    url.get(1).map(|m| m.as_str()).unwrap_or_default()
                );
            }
            if let Some(filename) = ri_filename_re().captures(whole) {
                let name = filename.get(1).map(|m| m.as_str()).unwrap_or("image");
                return format!("![{name}](attachment://{name})");
            }
            "[image]".to_string()
        })
        .into_owned();

    out = time_re().replace_all(&out, "$1").into_owned();
    out = out.replace("&nbsp;", " ");
    out
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn strip_tags(value: &str) -> String {
    tag_re().replace_all(value, "").to_string()
}

fn code_macro_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:structured-macro[^>]*ac:name="code"[^>]*>.*?<ac:plain-text-body>\s*<!\[CDATA\[(.*?)\]\]>\s*</ac:plain-text-body>.*?</ac:structured-macro>"#)
            .expect("valid code macro regex")
    })
}

fn language_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:parameter[^>]*ac:name="language"[^>]*>(.*?)</ac:parameter>"#)
            .expect("valid language regex")
    })
}

fn panel_macro_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:structured-macro[^>]*ac:name="(info|note|warning|tip|panel|expand)"[^>]*>.*?<ac:rich-text-body>(.*?)</ac:rich-text-body>.*?</ac:structured-macro>"#)
            .expect("valid panel regex")
    })
}

fn toc_macro_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:structured-macro[^>]*ac:name="toc"[^>]*>.*?</ac:structured-macro>"#)
            .expect("valid toc regex")
    })
}

fn jira_macro_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:structured-macro[^>]*ac:name="jira"[^>]*>.*?<ac:parameter[^>]*ac:name="key"[^>]*>(.*?)</ac:parameter>.*?</ac:structured-macro>"#)
            .expect("valid jira regex")
    })
}

fn include_macro_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:structured-macro[^>]*ac:name="include"[^>]*>.*?<ac:parameter[^>]*ac:name="[^"]*page"[^>]*>(.*?)</ac:parameter>.*?</ac:structured-macro>"#)
            .expect("valid include regex")
    })
}

fn parameter_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:parameter[^>]*>.*?</ac:parameter>"#).expect("valid parameter regex")
    })
}

fn inline_comment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)</?ac:inline-comment-marker[^>]*>"#).expect("valid inline comment regex")
    })
}

fn link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)<ac:link[^>]*>.*?</ac:link>"#).expect("valid link regex"))
}

fn link_body_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:link-body[^>]*>(.*?)</ac:link-body>"#)
            .expect("valid link body regex")
    })
}

fn content_title_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)ri:content-title="([^"]+)""#).expect("valid content title regex")
    })
}

fn image_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:image[^>]*>.*?</ac:image>"#).expect("valid image regex")
    })
}

fn ri_url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)ri:value="([^"]+)""#).expect("valid ri url regex"))
}

fn ri_filename_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)ri:filename="([^"]+)""#).expect("valid ri filename regex"))
}

fn task_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:task(?:\s[^>]*)?>(.*?)</ac:task>"#).expect("valid task regex")
    })
}

fn task_status_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:task-status(?:\s[^>]*)?>(.*?)</ac:task-status>"#)
            .expect("valid task status regex")
    })
}

fn task_body_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:task-body(?:\s[^>]*)?>(.*?)</ac:task-body>"#)
            .expect("valid task body regex")
    })
}

fn task_list_open_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:task-list(?:\s[^>]*)?>"#).expect("valid task list open regex")
    })
}

fn task_list_close_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)</ac:task-list>"#).expect("valid task list close regex"))
}

fn emoticon_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<ac:emoticon[^>]*ac:name="([^"]+)"[^>]*/?>"#)
            .expect("valid emoticon regex")
    })
}

fn self_closing_time_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<time[^>]*datetime="([^"]+)"[^>]*/>"#)
            .expect("valid self-closing time regex")
    })
}

fn time_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)<time[^>]*>(.*?)</time>"#).expect("valid time regex"))
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)<[^>]+>"#).expect("valid tag regex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_plain_text_excerpt() {
        let text = "Подписание контракта с поставщиком (импорт)\nОписание процесса как есть";
        let md = storage_to_markdown(text);
        assert!(md.contains("Подписание контракта"), "{md:?}");
    }

    #[test]
    fn converts_task_list_and_highlight() {
        let html = r#"<ac:task-list><ac:task><ac:task-status>incomplete</ac:task-status><ac:task-body>Cat walk</ac:task-body></ac:task></ac:task-list><p>@@@hl@@@Testing@@@endhl@@@</p>"#;
        let md = storage_to_markdown(html);
        assert!(md.contains("[ ] Cat walk"), "{md}");
        assert!(md.contains("Testing"), "{md}");
        assert!(!md.contains("@@@hl@@@"), "{md}");
    }

    #[test]
    fn keeps_self_closing_time() {
        let md = storage_to_markdown(r#"<p><time datetime="2023-02-20" /> done</p>"#);
        assert!(md.contains("2023-02-20"), "{md}");
    }

    #[test]
    fn converts_basic_storage() {
        let html = "<p>Hello <strong>world</strong></p>";
        let md = storage_to_markdown(html);
        assert!(md.contains("Hello **world**"));
    }

    #[test]
    fn converts_code_macro() {
        let html = r#"<ac:structured-macro ac:name="code"><ac:parameter ac:name="language">sql</ac:parameter><ac:plain-text-body><![CDATA[select 1 < 2;]]></ac:plain-text-body></ac:structured-macro>"#;
        let md = storage_to_markdown(html);
        assert!(md.contains("select 1 < 2;"), "{md}");
    }

    #[test]
    fn converts_info_macro() {
        let html = r#"<ac:structured-macro ac:name="info"><ac:rich-text-body><p>Note</p></ac:rich-text-body></ac:structured-macro>"#;
        let md = storage_to_markdown(html);
        assert!(md.contains("Note"));
    }
}
