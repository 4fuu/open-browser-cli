use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::time::sleep;

use super::loader::Plugin;
#[cfg(test)]
use super::loader::Step;
use crate::page::structure::{Node, PageData, parse_page_from_snapshot, parse_snapshot};
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

#[derive(Debug, Clone, Serialize)]
pub struct PluginRunSummary {
    pub plugin: String,
    pub session_id: String,
    pub steps_total: usize,
    pub steps_completed: usize,
    pub steps_skipped: usize,
    pub steps_failed: usize,
    pub page_updated: bool,
    pub steps: Vec<PluginStepSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginStepSummary {
    pub index: usize,
    pub action: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
enum StepState {
    Completed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone)]
struct StepOutcome {
    state: StepState,
    detail: String,
    page_updated: bool,
}

fn is_ms_value(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

pub async fn run_plugin(plugin: &Plugin, session_id: &str) -> Result<PluginRunSummary> {
    eprintln!(
        "plugin: running '{}' ({} steps) on session {session_id}",
        plugin.name,
        plugin.steps.len()
    );

    let mut summary = PluginRunSummary {
        plugin: plugin.name.clone(),
        session_id: session_id.to_string(),
        steps_total: plugin.steps.len(),
        steps_completed: 0,
        steps_skipped: 0,
        steps_failed: 0,
        page_updated: false,
        steps: Vec::with_capacity(plugin.steps.len()),
    };

    for (i, step) in plugin.steps.iter().enumerate() {
        eprintln!(
            "plugin: step {i}: action='{}' wait={:?}",
            step.action, step.wait
        );

        let outcome = match step.action.as_str() {
            "wait" => run_wait_step(step, session_id, i).await?,
            "click" => run_interactive_step(step, session_id, i, "click", None).await?,
            "type" => {
                run_interactive_step(step, session_id, i, "type", step.value.as_deref()).await?
            }
            "scroll" => {
                eprintln!("plugin: step {i}: scroll is not implemented yet, skipping");
                StepOutcome {
                    state: StepState::Skipped,
                    detail: "scroll is not implemented yet".into(),
                    page_updated: false,
                }
            }
            other => {
                eprintln!("plugin: step {i}: unknown action '{other}', skipping");
                StepOutcome {
                    state: StepState::Skipped,
                    detail: format!("unknown action '{other}'"),
                    page_updated: false,
                }
            }
        };

        summary.page_updated |= outcome.page_updated;
        match outcome.state {
            StepState::Completed => summary.steps_completed += 1,
            StepState::Skipped => summary.steps_skipped += 1,
            StepState::Failed => summary.steps_failed += 1,
        }
        summary.steps.push(PluginStepSummary {
            index: i,
            action: step.action.clone(),
            status: match outcome.state {
                StepState::Completed => "completed",
                StepState::Skipped => "skipped",
                StepState::Failed => "failed",
            }
            .into(),
            detail: outcome.detail,
        });
    }

    eprintln!("plugin: '{}' finished", plugin.name);
    Ok(summary)
}

async fn run_wait_step(
    step: &super::loader::Step,
    session_id: &str,
    index: usize,
) -> Result<StepOutcome> {
    let Some(wait_value) = step.wait.as_deref() else {
        return Ok(StepOutcome {
            state: StepState::Completed,
            detail: "no wait condition".into(),
            page_updated: false,
        });
    };

    if is_ms_value(wait_value) {
        let ms: u64 = wait_value.parse().unwrap();
        eprintln!("plugin: step {index}: sleeping {ms}ms");
        sleep(Duration::from_millis(ms)).await;
        return Ok(StepOutcome {
            state: StepState::Completed,
            detail: format!("slept {ms}ms"),
            page_updated: false,
        });
    }

    let timeout = step.timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_MS);
    let query = normalize_plugin_query(wait_value);
    eprintln!("plugin: step {index}: waiting for query '{query}' (timeout {timeout}ms)");

    match wait_for_query(session_id, &query, timeout).await? {
        Some(page_updated) => Ok(StepOutcome {
            state: StepState::Completed,
            detail: format!("matched query '{query}'"),
            page_updated,
        }),
        None => {
            eprintln!("plugin: step {index}: wait timed out, skipping");
            Ok(StepOutcome {
                state: StepState::Skipped,
                detail: format!("wait timed out for '{query}'"),
                page_updated: false,
            })
        }
    }
}

async fn run_interactive_step(
    step: &super::loader::Step,
    session_id: &str,
    index: usize,
    action: &str,
    value: Option<&str>,
) -> Result<StepOutcome> {
    if action == "type" && value.is_none() {
        eprintln!("plugin: step {index}: 'type' action missing value, skipping");
        return Ok(StepOutcome {
            state: StepState::Skipped,
            detail: "type action missing value".into(),
            page_updated: false,
        });
    }

    let Some(target) = prepare_interactive_target(step, session_id, index).await? else {
        return Ok(StepOutcome {
            state: StepState::Skipped,
            detail: "target not found".into(),
            page_updated: false,
        });
    };

    let params = match action {
        "click" => serde_json::json!({
            "session_id": session_id,
            "ref": target.ref_id,
        }),
        "type" => serde_json::json!({
            "session_id": session_id,
            "ref": target.ref_id,
            "text": value.unwrap_or_default(),
        }),
        _ => unreachable!("unsupported interactive action"),
    };

    Ok(send_plugin_action(Request::new(action, params), index, action).await)
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

async fn wait_for_query(session_id: &str, query: &str, timeout_ms: u64) -> Result<Option<bool>> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut observed_change = false;

    loop {
        let page = fetch_page(session_id).await?;
        if page_contains_query(&page, query) {
            return Ok(Some(observed_change));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        observed_change = true;
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

    for node in &page.nodes {
        for (element_id, candidate) in interactive_targets(node) {
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
    }

    None
}

fn page_contains_query(page: &PageData, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    page.nodes.iter().any(|node| {
        let mut haystack = String::new();
        for (id, candidate) in interactive_targets(node) {
            haystack.push_str(&id);
            haystack.push(' ');
            haystack.push_str(&candidate);
            haystack.push(' ');
        }
        haystack.push_str(&node_text(node));
        haystack.to_lowercase().contains(&query)
    })
}

fn interactive_targets(node: &Node) -> Vec<(String, String)> {
    let mut out = Vec::new();
    collect_interactive_targets(node, &mut out);
    out
}

fn collect_interactive_targets(node: &Node, out: &mut Vec<(String, String)>) {
    match node {
        Node::Link { id, text, href, .. } => {
            out.push((
                id.clone(),
                join_parts([Some(text.as_str()), href.as_deref()]),
            ));
        }
        Node::Button { id, text, .. } => {
            out.push((id.clone(), text.clone()));
        }
        Node::Input {
            id,
            input_type,
            placeholder,
            value,
            ..
        } => {
            out.push((
                id.clone(),
                join_parts([
                    Some(input_type.as_str()),
                    placeholder.as_deref(),
                    value.as_deref(),
                ]),
            ));
        }
        Node::Checkbox { id, text, .. } => {
            out.push((id.clone(), text.clone()));
        }
        Node::Radio { id, text, name, .. } => {
            out.push((
                id.clone(),
                join_parts([Some(text.as_str()), name.as_deref()]),
            ));
        }
        Node::Select {
            id, text, selected, ..
        } => {
            out.push((
                id.clone(),
                join_parts([Some(text.as_str()), selected.as_deref()]),
            ));
        }
        Node::Textarea {
            id,
            text,
            placeholder,
            ..
        } => {
            out.push((
                id.clone(),
                join_parts([Some(text.as_str()), placeholder.as_deref()]),
            ));
        }
        Node::Container { children, .. }
        | Node::List { children, .. }
        | Node::Item { children, .. }
        | Node::Table { children, .. }
        | Node::Row { children }
        | Node::Cell { children } => {
            for child in children {
                collect_interactive_targets(child, out);
            }
        }
        Node::Media {
            id,
            tag,
            media_state,
            ..
        } => {
            out.push((
                id.clone(),
                join_parts([Some(tag.as_str()), Some(media_state.as_str())]),
            ));
        }
        Node::Text { .. } | Node::Heading { .. } => {}
    }
}

fn node_text(node: &Node) -> String {
    match node {
        Node::Text { text, .. }
        | Node::Heading { text, .. }
        | Node::Button { text, .. }
        | Node::Checkbox { text, .. }
        | Node::Radio { text, .. }
        | Node::Select { text, .. }
        | Node::Textarea { text, .. } => text.clone(),
        Node::Link { text, href, .. } => join_parts([Some(text.as_str()), href.as_deref()]),
        Node::Input {
            input_type,
            placeholder,
            value,
            ..
        } => join_parts([
            Some(input_type.as_str()),
            placeholder.as_deref(),
            value.as_deref(),
        ]),
        Node::Container { children, .. }
        | Node::List { children, .. }
        | Node::Item { children, .. }
        | Node::Table { children, .. }
        | Node::Row { children }
        | Node::Cell { children } => children
            .iter()
            .map(node_text)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        Node::Media { tag, media_state, .. } => format!("{} ({})", tag, media_state),
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
    if let Some(captures) = contains_re.captures(trimmed)
        && let Some(value) = captures.get(1).or_else(|| captures.get(2))
    {
        return value.as_str().trim().to_string();
    }
    trimmed.to_string()
}

async fn send_plugin_action(req: Request, index: usize, action: &str) -> StepOutcome {
    match send_request(&req).await {
        Ok(response) if !response.ok => {
            let detail = response.error.unwrap_or_else(|| "unknown".into());
            eprintln!("plugin: step {index}: {action} failed ({detail})");
            StepOutcome {
                state: StepState::Failed,
                detail,
                page_updated: false,
            }
        }
        Err(error) => {
            eprintln!("plugin: step {index}: {action} error ({error})");
            StepOutcome {
                state: StepState::Failed,
                detail: error.to_string(),
                page_updated: false,
            }
        }
        Ok(response) => {
            let changed = response
                .data
                .as_ref()
                .and_then(|data| data.get("changed"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            StepOutcome {
                state: StepState::Completed,
                detail: format!("{action} completed"),
                page_updated: changed,
            }
        }
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
        let result = run_plugin(&plugin, "test-session-id").await.unwrap();
        assert_eq!(result.steps_total, 0);
        assert_eq!(result.steps_completed, 0);
        assert_eq!(result.steps_skipped, 0);
        assert_eq!(result.steps_failed, 0);
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
        let result = run_plugin(&plugin, "test-session-id").await.unwrap();
        assert!(start.elapsed() >= std::time::Duration::from_millis(80));
        assert_eq!(result.steps_completed, 1);
        assert_eq!(result.steps_skipped, 0);
        assert_eq!(result.steps_failed, 0);
        assert_eq!(result.steps[0].status, "completed");
    }

    #[tokio::test]
    async fn test_run_plugin_type_without_value_is_skipped() {
        let plugin = Plugin {
            name: "typer".to_string(),
            description: None,
            match_pattern: "*".to_string(),
            trigger: "manual".to_string(),
            steps: vec![Step {
                wait: Some("Search".to_string()),
                timeout: None,
                action: "type".to_string(),
                value: None,
            }],
        };
        let result = run_plugin(&plugin, "test-session-id").await.unwrap();
        assert_eq!(result.steps_skipped, 1);
        assert_eq!(result.steps[0].status, "skipped");
    }
}
