use anyhow::Result;
use regex::Regex;
use std::time::{Duration, Instant};
use tokio::time::sleep;

use super::loader::Plugin;
#[cfg(test)]
use super::loader::Step;
use crate::page::structure::{Element, PageData, parse_page_from_snapshot, parse_snapshot};
use crate::protocol::messages::{Request, actions};
use crate::transport::client::send_request;

const DEFAULT_STEP_TIMEOUT_MS: u64 = 5_000;
const POLL_INTERVAL_MS: u64 = 200;

#[derive(Debug, Clone)]
struct ResolvedTarget {
    element_id: String,
    ref_id: String,
    description: String,
}

fn is_ms_value(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

pub async fn run_plugin(plugin: &Plugin, session_id: &str) -> Result<()> {
    eprintln!(
        "plugin: running '{}' ({} steps) on session {session_id}",
        plugin.name,
        plugin.steps.len()
    );

    for (i, step) in plugin.steps.iter().enumerate() {
        eprintln!(
            "plugin: step {i}: action='{}' wait={:?}",
            step.action, step.wait
        );

        match step.action.as_str() {
            "wait" => {
                run_wait_step(step, session_id, i).await?;
            }
            "click" => {
                if let Some(target) = prepare_interactive_target(step, session_id, i).await? {
                    send_plugin_action(
                        Request::new(
                            actions::CLICK,
                            serde_json::json!({
                                "session_id": session_id,
                                "ref": target.ref_id,
                            }),
                        ),
                        i,
                        "click",
                    )
                    .await;
                }
            }
            "type" => {
                let Some(text) = step.value.as_deref() else {
                    eprintln!("plugin: step {i}: 'type' action missing value, skipping");
                    continue;
                };
                if let Some(target) = prepare_interactive_target(step, session_id, i).await? {
                    send_plugin_action(
                        Request::new(
                            actions::TYPE,
                            serde_json::json!({
                                "session_id": session_id,
                                "ref": target.ref_id,
                                "text": text,
                            }),
                        ),
                        i,
                        "type",
                    )
                    .await;
                }
            }
            "scroll" => {
                eprintln!("plugin: step {i}: scroll is not implemented yet, skipping");
            }
            other => {
                eprintln!("plugin: step {i}: unknown action '{other}', skipping");
            }
        }
    }

    eprintln!("plugin: '{}' finished", plugin.name);
    Ok(())
}

async fn run_wait_step(step: &super::loader::Step, session_id: &str, index: usize) -> Result<()> {
    let Some(wait_value) = step.wait.as_deref() else {
        return Ok(());
    };

    if is_ms_value(wait_value) {
        let ms: u64 = wait_value.parse().unwrap();
        eprintln!("plugin: step {index}: sleeping {ms}ms");
        sleep(Duration::from_millis(ms)).await;
        return Ok(());
    }

    let timeout = step.timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_MS);
    let query = normalize_plugin_query(wait_value);
    eprintln!("plugin: step {index}: waiting for query '{query}' (timeout {timeout}ms)");

    if wait_for_query(session_id, &query, timeout).await?.is_none() {
        eprintln!("plugin: step {index}: wait timed out, skipping");
    }

    Ok(())
}

async fn prepare_interactive_target(
    step: &super::loader::Step,
    session_id: &str,
    index: usize,
) -> Result<Option<ResolvedTarget>> {
    let Some(wait_value) = step.wait.as_deref() else {
        eprintln!("plugin: step {index}: action requires a target query in step.wait, skipping");
        return Ok(None);
    };

    if is_ms_value(wait_value) {
        eprintln!("plugin: step {index}: numeric wait cannot identify a target, skipping");
        return Ok(None);
    }

    let query = normalize_plugin_query(wait_value);
    let timeout = step.timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_MS);
    eprintln!("plugin: step {index}: resolving target '{query}'");

    let Some(target) = wait_for_interactive_target(session_id, &query, timeout).await? else {
        eprintln!("plugin: step {index}: target not found, skipping");
        return Ok(None);
    };

    eprintln!(
        "plugin: step {index}: resolved {} -> {}",
        target.element_id, target.description
    );
    Ok(Some(target))
}

async fn wait_for_query(session_id: &str, query: &str, timeout_ms: u64) -> Result<Option<()>> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        let page = fetch_page(session_id).await?;
        if page_contains_query(&page, query) {
            return Ok(Some(()));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

async fn wait_for_interactive_target(
    session_id: &str,
    query: &str,
    timeout_ms: u64,
) -> Result<Option<ResolvedTarget>> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        let page = fetch_page(session_id).await?;
        if let Some(target) = resolve_interactive_target(&page, query) {
            return Ok(Some(target));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

async fn fetch_page(session_id: &str) -> Result<PageData> {
    let response = send_request(&Request::new(
        actions::GET_PAGE,
        serde_json::json!({ "session_id": session_id }),
    ))
    .await?;

    if !response.ok {
        anyhow::bail!(
            "{}",
            response.error.unwrap_or_else(|| "Unknown error".into())
        );
    }

    let data = response
        .data
        .ok_or_else(|| anyhow::anyhow!("missing snapshot payload"))?;
    let snapshot = parse_snapshot(&data)?;
    parse_page_from_snapshot(&snapshot, None)
}

fn resolve_interactive_target(page: &PageData, query: &str) -> Option<ResolvedTarget> {
    let query = query.trim().to_lowercase();

    for element in &page.elements {
        let element_id = interactive_element_id(element)?;
        let candidate = interactive_candidate_text(element);
        let matches = element_id.eq_ignore_ascii_case(query.as_str())
            || candidate.to_lowercase().contains(&query);
        if !matches {
            continue;
        }
        let ref_id = page.element_refs.get(&element_id)?.clone();
        return Some(ResolvedTarget {
            element_id,
            ref_id,
            description: candidate,
        });
    }

    None
}

fn page_contains_query(page: &PageData, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    page.elements.iter().any(|element| {
        let mut haystack = String::new();
        if let Some(id) = interactive_element_id(element) {
            haystack.push_str(&id);
            haystack.push(' ');
        }
        haystack.push_str(&interactive_candidate_text(element));
        haystack.push(' ');
        haystack.push_str(&element_text(element));
        haystack.to_lowercase().contains(&query)
    })
}

fn interactive_element_id(element: &Element) -> Option<String> {
    match element {
        Element::Link { id, .. }
        | Element::Button { id, .. }
        | Element::Input { id, .. }
        | Element::Checkbox { id, .. }
        | Element::Radio { id, .. }
        | Element::Select { id, .. }
        | Element::Textarea { id, .. } => Some(id.clone()),
        Element::Text { .. }
        | Element::Heading { .. }
        | Element::List { .. }
        | Element::Table { .. } => None,
    }
}

fn interactive_candidate_text(element: &Element) -> String {
    match element {
        Element::Link { text, href, .. } => join_parts([Some(text.as_str()), href.as_deref()]),
        Element::Button { text, .. } => text.clone(),
        Element::Input {
            input_type,
            placeholder,
            value,
            ..
        } => join_parts([
            Some(input_type.as_str()),
            placeholder.as_deref(),
            value.as_deref(),
        ]),
        Element::Checkbox { text, .. } => text.clone(),
        Element::Radio { text, name, .. } => join_parts([Some(text.as_str()), name.as_deref()]),
        Element::Select { text, selected, .. } => {
            join_parts([Some(text.as_str()), selected.as_deref()])
        }
        Element::Textarea {
            text, placeholder, ..
        } => join_parts([Some(text.as_str()), placeholder.as_deref()]),
        Element::Text { .. }
        | Element::Heading { .. }
        | Element::List { .. }
        | Element::Table { .. } => String::new(),
    }
}

fn element_text(element: &Element) -> String {
    match element {
        Element::Text { text, .. }
        | Element::Button { text, .. }
        | Element::Heading { text, .. }
        | Element::Checkbox { text, .. }
        | Element::Textarea { text, .. }
        | Element::Select { text, .. }
        | Element::Radio { text, .. } => text.clone(),
        Element::Link { text, href, .. } => join_parts([Some(text.as_str()), href.as_deref()]),
        Element::Input {
            input_type,
            placeholder,
            value,
            ..
        } => join_parts([
            Some(input_type.as_str()),
            placeholder.as_deref(),
            value.as_deref(),
        ]),
        Element::List { items } => items.join(" "),
        Element::Table { rows } => rows
            .iter()
            .flat_map(|r| r.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn join_parts<const N: usize>(parts: [Option<&str>; N]) -> String {
    parts
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_plugin_query(query: &str) -> String {
    let trimmed = query.trim();
    let contains_re =
        Regex::new(r#"^[a-zA-Z0-9_-]+:contains\((?:"([^"]+)"|'([^']+)')\)$"#).unwrap();
    if let Some(captures) = contains_re.captures(trimmed) {
        if let Some(value) = captures.get(1).or_else(|| captures.get(2)) {
            return value.as_str().trim().to_string();
        }
    }
    trimmed.to_string()
}

async fn send_plugin_action(req: Request, index: usize, action: &str) {
    match send_request(&req).await {
        Ok(response) if !response.ok => {
            eprintln!(
                "plugin: step {index}: {action} failed ({})",
                response.error.as_deref().unwrap_or("unknown")
            );
        }
        Err(error) => {
            eprintln!("plugin: step {index}: {action} error ({error})");
        }
        Ok(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ms_value() {
        assert!(is_ms_value("500"));
        assert!(is_ms_value("0"));
        assert!(is_ms_value("3000"));
        assert!(!is_ms_value(""));
        assert!(!is_ms_value("button:contains('Accept')"));
        assert!(!is_ms_value("div.cookie"));
        assert!(!is_ms_value("50ms"));
    }

    #[test]
    fn test_normalize_plugin_query_extracts_contains_text() {
        assert_eq!(
            normalize_plugin_query("button:contains('Accept')"),
            "Accept"
        );
        assert_eq!(
            normalize_plugin_query("button:contains(\"Continue\")"),
            "Continue"
        );
        assert_eq!(normalize_plugin_query("e3"), "e3");
    }

    #[tokio::test]
    async fn test_run_plugin_empty_steps() {
        let plugin = Plugin {
            name: "empty".to_string(),
            description: None,
            match_pattern: "*".to_string(),
            trigger: "on_load".to_string(),
            steps: vec![],
        };
        let result = run_plugin(&plugin, "test-session-id").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_plugin_sleep_step() {
        let plugin = Plugin {
            name: "sleeper".to_string(),
            description: None,
            match_pattern: "*".to_string(),
            trigger: "on_load".to_string(),
            steps: vec![Step {
                wait: Some("100".to_string()),
                timeout: None,
                action: "wait".to_string(),
                value: None,
            }],
        };
        let start = std::time::Instant::now();
        let result = run_plugin(&plugin, "test-session-id").await;
        assert!(result.is_ok());
        assert!(start.elapsed() >= std::time::Duration::from_millis(80));
    }
}
