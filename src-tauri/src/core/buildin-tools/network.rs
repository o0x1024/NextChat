use anyhow::{bail, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::json;

use crate::core::domain::{ToolExecutionRequest, ToolExecutionResult};
use crate::core::tool_runtime::{domain_matches, truncate, ToolRuntime};

#[derive(Debug, Deserialize)]
struct WebFetchToolInput {
    url: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct WebSearchToolInput {
    query: String,
    allowed_domains: Option<Vec<String>>,
    blocked_domains: Option<Vec<String>>,
}

impl ToolRuntime {
    pub(crate) async fn run_web_fetch_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<WebFetchToolInput>(&request.input, "WebFetch")?;
        let normalized_url = if input.url.starts_with("http://") {
            format!("https://{}", input.url.trim_start_matches("http://"))
        } else {
            input.url.clone()
        };
        let response = self.http_client.get(&normalized_url).send().await?;
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        let output = json!({
            "url": normalized_url,
            "status": status,
            "prompt": input.prompt,
            "analysis": truncate(&body, 16_000),
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    pub(crate) async fn run_web_search_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<WebSearchToolInput>(&request.input, "WebSearch")?;
        if input.query.trim().len() < 2 {
            bail!("query must be at least 2 characters");
        }
        let url = reqwest::Url::parse_with_params(
            "https://duckduckgo.com/html/",
            &[("q", input.query.as_str())],
        )?;
        let response = self.http_client.get(url).send().await?;
        let html = response.text().await.unwrap_or_default();
        let result_re = Regex::new(r#"<a[^>]*class="result__a"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#)
            .expect("valid websearch regex");
        let tag_re = Regex::new(r"<[^>]+>").expect("valid strip-tags regex");
        let mut results = Vec::new();
        for capture in result_re.captures_iter(&html) {
            if results.len() >= 8 {
                break;
            }
            let url = capture.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let title_html = capture.get(2).map(|m| m.as_str()).unwrap_or("");
            let title = tag_re.replace_all(title_html, "").trim().to_string();
            if title.is_empty() || url.is_empty() {
                continue;
            }
            let host = reqwest::Url::parse(&url)
                .ok()
                .and_then(|value| value.host_str().map(str::to_string))
                .unwrap_or_default();
            if input.blocked_domains.as_ref().is_some_and(|domains| {
                domains
                    .iter()
                    .any(|domain| domain_matches(&host, domain.as_str()))
            }) {
                continue;
            }
            if input.allowed_domains.as_ref().is_some_and(|domains| {
                !domains
                    .iter()
                    .any(|domain| domain_matches(&host, domain.as_str()))
            }) {
                continue;
            }
            results.push(json!({
                "title": title,
                "url": url,
                "host": host,
            }));
        }
        if results.is_empty() {
            results.push(json!({
                "title": "No parsed search results; returning snippet",
                "url": "https://duckduckgo.com/html/",
                "host": "duckduckgo.com",
                "snippet": truncate(&html, 500),
            }));
        }
        let output = json!({
            "query": input.query,
            "results": results,
            "taskCardId": request.task_card_id,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }
}
