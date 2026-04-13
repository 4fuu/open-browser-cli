use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::json;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;
use url::Url;

use crate::page::structure::{
    Node, PageData, parse_page_from_snapshot, parse_snapshot, resolve_block, resolve_block_all,
    search_snapshot,
};
use crate::protocol::messages::{Request, Response, actions};
use crate::transport::client::send_request;

const NATIVE_HOST_NAME: &str = "com.browser_cli.relay";
const CHROME_EXTENSION_PLACEHOLDER: &str = "REPLACE_WITH_EXTENSION_ID";
const FIREFOX_EXTENSION_PLACEHOLDER: &str = "4fu@browser-cli";
const DEFAULT_PAGE_ALL_SETTLE_MS: u64 = 500;

#[derive(Debug, Clone, Serialize)]
struct ActionOutput {
    action: String,
    session_id: String,
    element_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    opened_session_id: Option<String>,
    changed: bool,
    navigated: bool,
    page_updated: bool,
    url: Option<String>,
    title: Option<String>,
    page: Option<PageData>,
}

#[derive(Debug, Clone, Serialize)]
struct WaitOutput {
    session_id: String,
    timed_out: bool,
    found: bool,
    page_updated: bool,
    waited_ms: Option<u64>,
    page: Option<PageData>,
}

#[derive(Debug, Clone, Copy)]
pub struct ActionOptions {
    pub fresh: bool,
    pub json_mode: bool,
    pub quiet: bool,
}

pub async fn open(url: &str, wait_after_load: u64, quiet: bool, json_mode: bool) -> Result<()> {
    let (session_id, opened_url) = open_session(url, wait_after_load).await?;

    if quiet {
        if json_mode {
            print_json(&json!({
                "session_id": session_id,
                "url": opened_url,
            }))?;
        } else {
            println!("Session {session_id} opened: {opened_url}");
        }
        return Ok(());
    }

    let page = resolve_page(&session_id, None, actions::GET_PAGE).await?;
    if json_mode {
        print_json(&json!({
            "session_id": session_id,
            "url": opened_url,
            "page": page,
        }))?;
    } else {
        println!("Session {session_id} opened: {opened_url}\n");
        println!("{}", crate::cli::output::format_page(&page, false, true));
    }
    Ok(())
}

async fn open_session(url: &str, wait_after_load: u64) -> Result<(String, String)> {
    let data = send_ok(Request::new(
        actions::OPEN,
        json!({ "url": url, "wait_after_load": wait_after_load }),
    ))
    .await?;
    let session_id = data
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let opened_url = data
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or(url)
        .to_string();

    run_on_load_plugins(&session_id, &opened_url).await;

    Ok((session_id, opened_url))
}

async fn run_on_load_plugins(session_id: &str, opened_url: &str) {
    match crate::plugin::loader::find_matching_plugins(opened_url) {
        Ok(matching) => {
            for plugin in matching {
                if plugin.trigger == "on_load" {
                    eprintln!("Running plugin: {}", plugin.name);
                    if let Err(err) = crate::plugin::runner::run_plugin(&plugin, session_id).await {
                        eprintln!("warning: auto plugin '{}' failed: {err}", plugin.name);
                    }
                }
            }
        }
        Err(err) => {
            eprintln!("warning: failed to load auto plugins: {err}");
        }
    }
}

pub fn setup(
    browser: &str,
    extension_id: Option<&str>,
    manifest_path: Option<&Path>,
) -> Result<()> {
    validate_setup_args(browser, extension_id)?;

    #[cfg(target_os = "windows")]
    let use_registry = manifest_path.is_none();
    let manifest_path = match manifest_path {
        Some(p) => p.to_path_buf(),
        None => native_host_manifest_path(browser)?,
    };
    let relay_path = std::env::current_exe()?.canonicalize()?;
    let manifest = build_native_host_manifest(browser, &relay_path, extension_id);

    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    #[cfg(target_os = "windows")]
    if use_registry {
        write_windows_registry(browser, &manifest_path)?;
    }

    println!("Wrote native host manifest: {}", manifest_path.display());
    Ok(())
}

pub fn teardown(browser: &str, manifest_path: Option<&Path>) -> Result<()> {
    #[cfg(target_os = "windows")]
    let use_registry = manifest_path.is_none();
    let manifest_path = match manifest_path {
        Some(p) => p.to_path_buf(),
        None => native_host_manifest_path(browser)?,
    };

    if manifest_path.exists() {
        fs::remove_file(&manifest_path)?;
        println!("Removed native host manifest: {}", manifest_path.display());
    } else {
        println!("Manifest not found, skipping: {}", manifest_path.display());
    }

    #[cfg(target_os = "windows")]
    if use_registry {
        delete_windows_registry(browser)?;
    }

    Ok(())
}

pub async fn close(session_id: Option<&str>, close_all: bool, json_mode: bool) -> Result<()> {
    let params = if close_all {
        json!({ "all": true })
    } else {
        let session_id = session_id
            .ok_or_else(|| anyhow::anyhow!("session_id is required unless --all is used"))?;
        json!({ "session_id": session_id })
    };

    let data = send_ok(Request::new(actions::CLOSE, params)).await?;
    let closed = data
        .get("closed")
        .and_then(|value| value.as_u64())
        .unwrap_or(if close_all { 0 } else { 1 });

    if json_mode {
        print_json(&json!({ "closed": closed }))?;
    } else if close_all {
        println!("Closed {closed} session(s).");
    } else {
        println!("Session {} closed.", session_id.unwrap_or_default());
    }
    Ok(())
}

pub async fn list(json_mode: bool) -> Result<()> {
    let data = send_ok(Request::new(actions::LIST, json!({}))).await?;
    let sessions = data.get("sessions").unwrap_or(&data);
    if json_mode {
        print_json(&json!({ "sessions": sessions }))?;
    } else {
        println!("{}", crate::cli::output::format_session_list(sessions));
    }
    Ok(())
}

pub async fn page(
    session_id: &str,
    page_num: Option<u32>,
    next: bool,
    prev: bool,
    all: bool,
    settle_ms: Option<u64>,
    fresh: bool,
    json_mode: bool,
    verbose: bool,
) -> Result<()> {
    let page_data = if all {
        fetch_all_pages(
            session_id,
            fresh,
            settle_ms.unwrap_or(DEFAULT_PAGE_ALL_SETTLE_MS),
        )
        .await?
    } else {
        let action = if fresh {
            actions::GET_PAGE_FRESH
        } else {
            actions::GET_PAGE
        };
        let snapshot = fetch_snapshot(session_id, action).await?;
        let resolved_page = resolve_requested_page(&snapshot, page_num, next, prev);
        let snapshot = if let Some(target_page) = resolved_page {
            send_ok(Request::new(
                actions::SCROLL,
                json!({
                    "session_id": session_id,
                    "top": scroll_top_for_page(&snapshot, target_page),
                }),
            ))
            .await?;
            fetch_snapshot(session_id, actions::GET_PAGE_FRESH).await?
        } else {
            snapshot
        };
        parse_page_from_snapshot(&snapshot, resolved_page)?
    };
    println!("{}", crate::cli::output::format_page(&page_data, json_mode, verbose));
    Ok(())
}

pub async fn click(
    session_id: &str,
    target: &str,
    page_num: Option<u32>,
    new_session: bool,
    options: ActionOptions,
) -> Result<()> {
    let page = resolve_page(
        session_id,
        page_num,
        if options.fresh {
            actions::GET_PAGE_FRESH
        } else {
            actions::GET_PAGE
        },
    )
    .await?;
    let (element_key, ref_id) = resolve_element_target(target, &page, session_id, page_num)?;

    if new_session {
        let href = link_href_by_element_id(&page, &element_key).ok_or_else(|| {
            anyhow::anyhow!("element is not a link or does not have an href: {element_key}")
        })?;
        let url = resolve_link_url(&page.url, href)?;
        let (new_session_id, opened_url) = open_session(&url, 3000).await?;
        if options.json_mode {
            print_json(&json!({
                "action": "click_new_session",
                "source_session_id": session_id,
                "element_id": element_key,
                "session_id": new_session_id,
                "url": opened_url,
            }))?;
        } else {
            println!("Session {new_session_id} opened: {opened_url}");
        }
        return Ok(());
    }

    let click_data = send_ok(Request::new(
        actions::CLICK,
        json!({ "session_id": session_id, "ref": ref_id }),
    ))
    .await?;
    let opened_session_id = click_data
        .get("new_session_id")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let output_session_id = opened_session_id.as_deref().unwrap_or(session_id);

    let navigated = click_data
        .get("navigated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let changed = click_data
        .get("changed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let should_fetch_page = !options.quiet;
    let updated = if should_fetch_page {
        if opened_session_id.is_some() {
            Some(resolve_page(output_session_id, None, actions::GET_PAGE).await?)
        } else {
            Some(fetch_action_page(output_session_id, page_num, navigated).await?)
        }
    } else {
        None
    };
    let output = ActionOutput {
        action: "click".into(),
        session_id: session_id.to_string(),
        element_id: element_key.clone(),
        opened_session_id: opened_session_id.clone(),
        changed,
        navigated,
        page_updated: changed || navigated || opened_session_id.is_some(),
        url: click_data
            .get("url")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        title: click_data
            .get("title")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        page: updated.clone(),
    };

    if options.json_mode {
        print_json(&output)?;
    } else if options.quiet {
        if let Some(opened_session_id) = opened_session_id {
            println!("click opened session {opened_session_id}: {element_key}");
        } else {
            println!("click ok: {element_key}");
        }
    } else {
        if let Some(opened_session_id) = &output.opened_session_id {
            println!("Session {opened_session_id} opened from click: {element_key}\n");
        }
        println!(
            "{}",
            crate::cli::output::format_page(updated.as_ref().expect("page fetched"), false, true)
        );
    }
    Ok(())
}

fn resolve_element_target(
    target: &str,
    page: &PageData,
    session_id: &str,
    page_num: Option<u32>,
) -> Result<(String, String)> {
    if let Some(element_key) = normalize_element_target(target) {
        let ref_id = page
            .element_refs
            .get(&element_key)
            .cloned()
            .ok_or_else(|| element_lookup_error(session_id, page_num, &element_key))?;
        return Ok((element_key, ref_id));
    }

    let found = find_interactive_by_query(page, target).ok_or_else(|| {
        anyhow::anyhow!(
            "no interactive element matching \"{target}\" found on the current page. \
             Run `browser-cli page {session_id}` to see available elements."
        )
    })?;

    let ref_id = page
        .element_refs
        .get(&found)
        .cloned()
        .ok_or_else(|| element_lookup_error(session_id, page_num, &found))?;

    Ok((found, ref_id))
}

fn normalize_element_target(target: &str) -> Option<String> {
    if let Some(rest) = target.strip_prefix('e')
        && !rest.is_empty()
        && rest.chars().all(|ch| ch.is_ascii_digit())
    {
        return Some(format!("e{rest}"));
    }

    if target.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(format!("e{target}"));
    }

    None
}

fn normalize_text_target(target: &str) -> Option<String> {
    if let Some(rest) = target.strip_prefix('t')
        && !rest.is_empty()
        && rest.chars().all(|ch| ch.is_ascii_digit())
    {
        return Some(format!("t{rest}"));
    }

    if target.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(format!("t{target}"));
    }

    None
}

fn normalize_block_target(target: &str) -> Option<String> {
    if let Some(rest) = target.strip_prefix('b')
        && !rest.is_empty()
        && rest.chars().all(|ch| ch.is_ascii_digit())
    {
        return Some(format!("b{rest}"));
    }

    if target.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(format!("b{target}"));
    }

    None
}

fn find_interactive_by_query(page: &PageData, query: &str) -> Option<String> {
    let needle = query.to_lowercase();
    find_interactive_recursive(&page.nodes, &needle).or_else(|| {
        page.full_blocks.values().find_map(|block| match block {
            crate::page::structure::StoredBlock::List { items } => {
                find_interactive_recursive(items, &needle)
            }
            crate::page::structure::StoredBlock::Table { rows } => {
                find_interactive_recursive(rows, &needle)
            }
        })
    })
}

fn find_interactive_recursive(nodes: &[Node], needle: &str) -> Option<String> {
    for node in nodes {
        match node {
            Node::Link { id, text, href, .. } => {
                if text.to_lowercase().contains(needle)
                    || href
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(needle)
                {
                    return Some(id.clone());
                }
            }
            Node::Button { id, text, .. } => {
                if text.to_lowercase().contains(needle) {
                    return Some(id.clone());
                }
            }
            Node::Input {
                id,
                placeholder,
                value,
                ..
            } => {
                if placeholder
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(needle)
                    || value
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(needle)
                {
                    return Some(id.clone());
                }
            }
            Node::Checkbox { id, text, .. }
            | Node::Radio { id, text, .. }
            | Node::Select { id, text, .. }
            | Node::Textarea { id, text, .. } => {
                if text.to_lowercase().contains(needle) {
                    return Some(id.clone());
                }
            }
            Node::Container { children, .. }
            | Node::List { children, .. }
            | Node::Item { children, .. }
            | Node::Table { children, .. }
            | Node::Row { children }
            | Node::Cell { children } => {
                if let Some(found) = find_interactive_recursive(children, needle) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

fn link_href_by_element_id<'a>(page: &'a PageData, element_id: &str) -> Option<&'a str> {
    link_href_by_nodes(&page.nodes, element_id).or_else(|| {
        page.full_blocks.values().find_map(|block| match block {
            crate::page::structure::StoredBlock::List { items } => {
                link_href_in_nodes(items, element_id)
            }
            crate::page::structure::StoredBlock::Table { rows } => {
                link_href_in_nodes(rows, element_id)
            }
        })
    })
}

fn link_href_by_nodes<'a>(nodes: &'a [Node], element_id: &str) -> Option<&'a str> {
    for node in nodes {
        match node {
            Node::Link { id, href, .. } if id == element_id => return href.as_deref(),
            Node::Container { children, .. }
            | Node::List { children, .. }
            | Node::Item { children, .. }
            | Node::Table { children, .. }
            | Node::Row { children }
            | Node::Cell { children } => {
                if let Some(href) = link_href_by_nodes(children, element_id) {
                    return Some(href);
                }
            }
            _ => {}
        }
    }
    None
}

fn link_href_in_nodes<'a>(nodes: &'a [Node], element_id: &str) -> Option<&'a str> {
    for node in nodes {
        match node {
            Node::Link { id, href, .. } if id == element_id => return href.as_deref(),
            Node::Container { children, .. }
            | Node::List { children, .. }
            | Node::Item { children, .. }
            | Node::Table { children, .. }
            | Node::Row { children }
            | Node::Cell { children } => {
                if let Some(href) = link_href_in_nodes(children, element_id) {
                    return Some(href);
                }
            }
            _ => {}
        }
    }
    None
}

fn resolve_link_url(base_url: &str, href: &str) -> Result<String> {
    if let Ok(url) = Url::parse(href) {
        return Ok(url.into());
    }

    let base = Url::parse(base_url)
        .map_err(|err| anyhow::anyhow!("failed to parse current page url '{base_url}': {err}"))?;
    let joined = base.join(href).map_err(|err| {
        anyhow::anyhow!("failed to resolve link '{href}' against '{base_url}': {err}")
    })?;
    Ok(joined.into())
}

pub async fn type_text(
    session_id: &str,
    target: &str,
    text: &str,
    page_num: Option<u32>,
    options: ActionOptions,
) -> Result<()> {
    let page = resolve_page(
        session_id,
        page_num,
        if options.fresh {
            actions::GET_PAGE_FRESH
        } else {
            actions::GET_PAGE
        },
    )
    .await?;
    let (element_key, ref_id) = resolve_element_target(target, &page, session_id, page_num)?;

    let type_data = send_ok(Request::new(
        actions::TYPE,
        json!({ "session_id": session_id, "ref": ref_id, "text": text }),
    ))
    .await?;

    let navigated = type_data
        .get("navigated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let changed = type_data
        .get("changed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let should_fetch_page = !options.quiet;
    let updated = if should_fetch_page {
        Some(fetch_action_page(session_id, page_num, navigated).await?)
    } else {
        None
    };
    let output = ActionOutput {
        action: "type".into(),
        session_id: session_id.to_string(),
        element_id: element_key.clone(),
        opened_session_id: None,
        changed,
        navigated,
        page_updated: changed || navigated,
        url: type_data
            .get("url")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        title: type_data
            .get("title")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        page: updated.clone(),
    };

    if options.json_mode {
        print_json(&output)?;
    } else if options.quiet {
        println!("type ok: {element_key}");
    } else {
        println!(
            "{}",
            crate::cli::output::format_page(updated.as_ref().expect("page fetched"), false, true)
        );
    }
    Ok(())
}

pub async fn search(session_id: &str, query: &str, fresh: bool, json_mode: bool, verbose: bool) -> Result<()> {
    let snapshot = fetch_snapshot(
        session_id,
        if fresh {
            actions::GET_PAGE_FRESH
        } else {
            actions::SEARCH
        },
    )
    .await?;
    let results = search_snapshot(&snapshot, query);
    println!(
        "{}",
        crate::cli::output::format_search_results(&results, json_mode, verbose)
    );
    Ok(())
}

pub async fn wait(
    session_id: &str,
    for_text: Option<&str>,
    timeout: Option<u64>,
    quiet: bool,
    json_mode: bool,
) -> Result<()> {
    let timeout_ms = timeout.unwrap_or(30_000);

    if let Some(query) = for_text {
        return wait_for_text(session_id, query, timeout_ms, quiet, json_mode).await;
    }

    let response = send_request(&Request::new(
        actions::WAIT,
        json!({
            "session_id": session_id,
            "timeout": timeout_ms
        }),
    ))
    .await?;

    let wait_output = if response.ok {
        let resp = response.data.unwrap_or_default();
        let page = if !quiet {
            Some(fetch_action_page(session_id, None, true).await?)
        } else {
            None
        };
        WaitOutput {
            session_id: session_id.to_string(),
            timed_out: false,
            found: true,
            page_updated: resp
                .get("changed")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            waited_ms: resp.get("waited_ms").and_then(|value| value.as_u64()),
            page,
        }
    } else if is_wait_timeout_error(&response) {
        WaitOutput {
            session_id: session_id.to_string(),
            timed_out: true,
            found: false,
            page_updated: false,
            waited_ms: Some(timeout_ms),
            page: None,
        }
    } else {
        return response.into_result().map(|_| ());
    };

    if json_mode {
        print_json(&wait_output)?;
    } else if wait_output.timed_out {
        println!("Wait timed out after {timeout_ms}ms.");
    } else if quiet {
        println!("wait ok");
    } else if let Some(page) = wait_output.page.as_ref() {
        println!("{}", crate::cli::output::format_page(page, false, true));
    } else {
        println!("Page reached a stable state.");
    }
    Ok(())
}

async fn wait_for_text(
    session_id: &str,
    query: &str,
    timeout_ms: u64,
    quiet: bool,
    json_mode: bool,
) -> Result<()> {
    let start = std::time::Instant::now();
    let poll_interval = std::time::Duration::from_millis(500);

    loop {
        let snapshot = fetch_snapshot(session_id, actions::GET_PAGE_FRESH).await?;
        let results = search_snapshot(&snapshot, query);

        if !results.matches.is_empty() {
            let page = if !quiet {
                Some(parse_page_from_snapshot(&snapshot, None)?)
            } else {
                None
            };
            let waited_ms = start.elapsed().as_millis() as u64;

            let output = WaitOutput {
                session_id: session_id.to_string(),
                timed_out: false,
                found: true,
                page_updated: true,
                waited_ms: Some(waited_ms),
                page,
            };

            if json_mode {
                print_json(&output)?;
            } else if quiet {
                println!("wait ok: \"{}\" found", query);
            } else if let Some(page) = output.page.as_ref() {
                println!("{}", crate::cli::output::format_page(page, false, true));
            }
            return Ok(());
        }

        if start.elapsed().as_millis() as u64 >= timeout_ms {
            let output = WaitOutput {
                session_id: session_id.to_string(),
                timed_out: true,
                found: false,
                page_updated: false,
                waited_ms: Some(timeout_ms),
                page: None,
            };
            if json_mode {
                print_json(&output)?;
            } else {
                println!(
                    "Wait timed out after {timeout_ms}ms: \"{}\" not found.",
                    query
                );
            }
            return Ok(());
        }

        tokio::time::sleep(poll_interval).await;
    }
}

pub async fn text(
    session_id: &str,
    text_id: &str,
    page_num: Option<u32>,
    fresh: bool,
    json_mode: bool,
) -> Result<()> {
    let text_id = normalize_text_target(text_id).unwrap_or_else(|| text_id.to_string());
    let page = resolve_page(
        session_id,
        page_num,
        if fresh {
            actions::GET_PAGE_FRESH
        } else {
            actions::GET_TEXT
        },
    )
    .await?;
    let full_text = page
        .full_texts
        .get(&text_id)
        .cloned()
        .ok_or_else(|| text_lookup_error(session_id, page_num, &text_id))?;

    if json_mode {
        print_json(&json!({
            "session_id": session_id,
            "text_id": text_id,
            "text": full_text,
        }))?;
    } else {
        println!("{full_text}");
    }
    Ok(())
}

pub async fn block(
    session_id: &str,
    block_id: &str,
    source_page: Option<u32>,
    page_num: Option<u32>,
    all: bool,
    fresh: bool,
    json_mode: bool,
    verbose: bool,
) -> Result<()> {
    let block_id = normalize_block_target(block_id).unwrap_or_else(|| block_id.to_string());
    let page = resolve_page(
        session_id,
        source_page,
        if fresh {
            actions::GET_PAGE_FRESH
        } else {
            actions::GET_PAGE
        },
    )
    .await?;

    if all {
        let block = resolve_block_all(&page, &block_id)
            .ok_or_else(|| block_lookup_error(session_id, source_page, &block_id))?;
        println!("{}", crate::cli::output::format_block(&block, json_mode, verbose));
    } else {
        let block = resolve_block(&page, &block_id, page_num)
            .ok_or_else(|| block_lookup_error(session_id, source_page, &block_id))?;
        println!("{}", crate::cli::output::format_block(&block, json_mode, verbose));
    }
    Ok(())
}

pub async fn view(
    session_id: &str,
    target: &str,
    page_num: Option<u32>,
    fresh: bool,
    json_mode: bool,
    verbose: bool,
) -> Result<()> {
    use crate::page::structure::extract_view;

    let page = resolve_page(
        session_id,
        page_num,
        if fresh {
            actions::GET_PAGE_FRESH
        } else {
            actions::GET_PAGE
        },
    )
    .await?;

    let view = extract_view(&page, target, verbose)?;
    println!("{}", crate::cli::output::format_view(&view, json_mode, verbose));
    Ok(())
}

fn sanitize_filename(raw: &str) -> String {
    let name = std::path::Path::new(raw)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download")
        .to_string();

    let name = name.replace(['/', '\\'], "_");

    if name.is_empty() || name == "." || name == ".." {
        "download".to_string()
    } else {
        name
    }
}

/// Check whether a target string looks like a URL (contains `://`).
fn is_url_target(target: &str) -> bool {
    target.contains("://")
}

/// Resolve a download target that looks like an element ID (`e3`, `3`) into an
/// absolute URL by looking up the element in the cached page snapshot.
///
/// Returns `(element_key, resolved_url)` on success.
fn resolve_element_to_url(
    target: &str,
    snapshot: &crate::protocol::messages::RawSnapshot,
    page: &PageData,
) -> Result<(String, String)> {
    let element_key = normalize_element_target(target).ok_or_else(|| {
        anyhow::anyhow!("'{target}' is not a valid element ID or URL")
    })?;

    let ref_id = page.element_refs.get(&element_key).ok_or_else(|| {
        anyhow::anyhow!(
            "element {element_key} not found on the current page. \
             Run `browser-cli page <session>` to see available elements."
        )
    })?;

    // Find the raw node matching the ref_id and extract href or src
    let raw_node = snapshot
        .nodes
        .iter()
        .find(|n| n.ref_id == *ref_id)
        .ok_or_else(|| {
            anyhow::anyhow!("internal error: ref {ref_id} for {element_key} not in snapshot")
        })?;

    let url_attr = raw_node
        .attrs
        .get("href")
        .or_else(|| raw_node.attrs.get("src"))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "element {element_key} has no downloadable URL (no href or src attribute)"
            )
        })?;

    let resolved = resolve_link_url(&page.url, url_attr)?;
    Ok((element_key, resolved))
}

pub async fn download(
    session_id: &str,
    target: &str,
    output: Option<&str>,
    json_mode: bool,
) -> Result<()> {
    use base64::Engine;

    // Resolve the target: if it looks like an element ID, look up its URL from
    // the snapshot; if it's already a URL, pass it through directly.
    let download_url = if is_url_target(target) {
        target.to_string()
    } else {
        let snapshot = fetch_snapshot(session_id, actions::GET_PAGE).await?;
        let page = parse_page_from_snapshot(&snapshot, None)?;
        let (_element_key, url) = resolve_element_to_url(target, &snapshot, &page)?;
        url
    };

    let data = send_ok(Request::new(
        actions::DOWNLOAD,
        json!({
            "session_id": session_id,
            "target": download_url,
        }),
    ))
    .await?;

    let b64 = data
        .get("data")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let filename = data
        .get("filename")
        .and_then(|v| v.as_str())
        .unwrap_or("download");
    let content_type = data
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("application/octet-stream");
    let size = data
        .get("size")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| anyhow::anyhow!("failed to decode base64 data: {e}"))?;

    let out_path = match output {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(sanitize_filename(filename)),
    };

    fs::write(&out_path, &bytes)?;

    if json_mode {
        print_json(&json!({
            "path": out_path.display().to_string(),
            "content_type": content_type,
            "size": size,
        }))?;
    } else {
        println!(
            "Downloaded {} ({}, {} bytes)",
            out_path.display(),
            content_type,
            size,
        );
    }

    Ok(())
}

pub async fn plugin(name: &str, session_id: &str, json_mode: bool) -> Result<()> {
    let plugin = crate::plugin::loader::load_plugin(name)?;
    let summary = crate::plugin::runner::run_plugin(&plugin, session_id).await?;
    if json_mode {
        print_json(&summary)?;
    } else {
        println!(
            "Plugin '{}' finished: {}/{} completed, {} skipped, {} failed.",
            summary.plugin,
            summary.steps_completed,
            summary.steps_total,
            summary.steps_skipped,
            summary.steps_failed
        );
    }
    Ok(())
}

pub fn plugin_list(json_mode: bool) -> Result<()> {
    let plugins = crate::plugin::loader::list_plugins()?;
    if json_mode {
        print_json(&json!({ "plugins": plugins }))?;
    } else if plugins.is_empty() {
        println!("No plugins installed.");
    } else {
        for p in &plugins {
            let desc = p.description.as_deref().unwrap_or("-");
            println!(
                "{} — {} (trigger: {}, match: {})",
                p.name, desc, p.trigger, p.match_pattern
            );
        }
    }
    Ok(())
}

pub async fn screenshot(
    session_id: &str,
    output: Option<&str>,
    full_page: bool,
    quality: Option<u32>,
    json_mode: bool,
) -> Result<()> {
    use base64::Engine as _;
    use std::time::{SystemTime, UNIX_EPOCH};

    if full_page {
        eprintln!("Warning: --full-page is not yet supported; capturing viewport only.");
    }

    let mut params = json!({ "session_id": session_id, "full_page": false });
    if let Some(q) = quality {
        params["quality"] = json!(q);
    }

    let data = send_ok(Request::new(actions::SCREENSHOT, params)).await?;

    let image_b64 = data
        .get("image")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing image data in response"))?;
    let format = data
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("png");

    let image_bytes = base64::engine::general_purpose::STANDARD
        .decode(image_b64)
        .map_err(|e| anyhow::anyhow!("failed to decode base64 image: {e}"))?;

    let extension = if format == "jpeg" { "jpg" } else { "png" };
    let output_path = match output {
        Some(p) => PathBuf::from(p),
        None => {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            PathBuf::from(format!("screenshot-{ts}.{extension}"))
        }
    };

    fs::write(&output_path, &image_bytes)?;

    let size_bytes = image_bytes.len();

    if json_mode {
        print_json(&json!({
            "session_id": session_id,
            "path": output_path.display().to_string(),
            "format": format,
            "size_bytes": size_bytes,
        }))?;
    } else {
        println!(
            "Screenshot saved: {} ({} bytes, {})",
            output_path.display(),
            size_bytes,
            format
        );
    }

    Ok(())
}

async fn fetch_snapshot(
    session_id: &str,
    action: &str,
) -> Result<crate::protocol::messages::RawSnapshot> {
    let data = send_ok(Request::new(action, json!({ "session_id": session_id }))).await?;
    parse_snapshot(&data)
}

async fn resolve_page(
    session_id: &str,
    page_num: Option<u32>,
    action: &str,
) -> Result<crate::page::structure::PageData> {
    let snapshot = fetch_snapshot(session_id, action).await?;
    parse_page_from_snapshot(&snapshot, page_num)
}

async fn send_ok(req: Request) -> Result<serde_json::Value> {
    send_request(&req).await?.into_result()
}

async fn scroll_session_to(session_id: &str, top: f64) -> Result<()> {
    send_ok(Request::new(
        actions::SCROLL,
        json!({
            "session_id": session_id,
            "top": top,
        }),
    ))
    .await?;
    Ok(())
}

async fn fetch_all_pages(session_id: &str, fresh: bool, settle_ms: u64) -> Result<PageData> {
    let initial_action = if fresh {
        actions::GET_PAGE_FRESH
    } else {
        actions::GET_PAGE
    };
    let initial_snapshot = fetch_snapshot(session_id, initial_action).await?;
    let original_scroll_top = initial_snapshot.scroll.top;

    let result = fetch_all_pages_from_snapshot(session_id, initial_snapshot, settle_ms).await;
    let restore_result = scroll_session_to(session_id, original_scroll_top).await;

    match (result, restore_result) {
        (Ok(page), Ok(())) => Ok(page),
        (Ok(_), Err(err)) => {
            Err(err.context("captured full page but failed to restore scroll position"))
        }
        (Err(err), Ok(())) => Err(err),
        (Err(err), Err(restore_err)) => Err(err.context(format!(
            "also failed to restore scroll position: {restore_err}"
        ))),
    }
}

async fn fetch_all_pages_from_snapshot(
    session_id: &str,
    mut snapshot: crate::protocol::messages::RawSnapshot,
    settle_ms: u64,
) -> Result<PageData> {
    let mut pages = Vec::new();
    let mut target_page = 1;

    loop {
        let total_pages = total_pages_for_snapshot(&snapshot);
        if target_page > total_pages {
            break;
        }

        eprintln!("Scrolling {target_page}/{total_pages}...");
        scroll_session_to(session_id, scroll_top_for_page(&snapshot, target_page)).await?;
        sleep(Duration::from_millis(settle_ms)).await;
        snapshot = fetch_snapshot(session_id, actions::GET_PAGE_FRESH).await?;
        pages.push(parse_page_from_snapshot(&snapshot, Some(target_page))?);
        target_page += 1;
    }

    Ok(aggregate_pages(pages))
}

fn aggregate_pages(pages: Vec<PageData>) -> PageData {
    if pages.is_empty() {
        return PageData {
            url: String::new(),
            title: String::new(),
            current_page: 1,
            total_pages: 1,
            truncated: false,
            shown: 0,
            total: 0,
            nodes: Vec::new(),
            element_refs: Default::default(),
            full_texts: Default::default(),
            full_blocks: Default::default(),
        };
    }

    let mut pages = pages.into_iter();
    let first_page = pages.next().expect("pages checked non-empty");
    let mut aggregated_nodes = first_page.nodes;

    for page in pages {
        append_page_nodes(&mut aggregated_nodes, page.nodes);
    }

    reassign_page_node_ids(&mut aggregated_nodes);
    let total = count_page_nodes(&aggregated_nodes);

    PageData {
        url: first_page.url,
        title: first_page.title,
        current_page: 1,
        total_pages: 1,
        truncated: false,
        shown: total,
        total,
        nodes: aggregated_nodes,
        element_refs: Default::default(),
        full_texts: Default::default(),
        full_blocks: Default::default(),
    }
}

fn append_page_nodes(aggregated: &mut Vec<Node>, incoming: Vec<Node>) {
    if aggregated.is_empty() {
        aggregated.extend(incoming);
        return;
    }

    let seen_keys: HashSet<String> = aggregated.iter().map(node_identity_key).collect();
    let mut skipping_prefix_duplicates = true;

    for node in incoming {
        let key = node_identity_key(&node);
        if skipping_prefix_duplicates && seen_keys.contains(&key) {
            continue;
        }

        skipping_prefix_duplicates = false;
        aggregated.push(node);
    }
}

fn node_identity_key(node: &Node) -> String {
    match node {
        Node::Container {
            tag,
            role,
            class_name,
            children,
        } => format!(
            "container|{tag}|{}|{}|{}",
            role.as_deref().unwrap_or_default(),
            class_name.as_deref().unwrap_or_default(),
            child_identity_keys(children)
        ),
        Node::Text { text, .. } => format!("text|{text}"),
        Node::Heading { level, text } => format!("heading|{level}|{text}"),
        Node::Link {
            text,
            href,
            class_name,
            ..
        } => format!(
            "link|{text}|{}|{}",
            href.as_deref().unwrap_or_default(),
            class_name.as_deref().unwrap_or_default()
        ),
        Node::Button {
            text, class_name, ..
        } => format!(
            "button|{text}|{}",
            class_name.as_deref().unwrap_or_default()
        ),
        Node::Input {
            input_type,
            placeholder,
            value,
            disabled,
            ..
        } => format!(
            "input|{input_type}|{}|{}|{disabled}",
            placeholder.as_deref().unwrap_or_default(),
            value.as_deref().unwrap_or_default()
        ),
        Node::Checkbox { text, checked, .. } => format!("checkbox|{text}|{checked}"),
        Node::Radio {
            text,
            name,
            selected,
            ..
        } => format!(
            "radio|{text}|{}|{selected}",
            name.as_deref().unwrap_or_default()
        ),
        Node::Select {
            text,
            selected,
            disabled,
            ..
        } => format!(
            "select|{text}|{}|{disabled}",
            selected.as_deref().unwrap_or_default()
        ),
        Node::Textarea {
            text,
            placeholder,
            disabled,
            ..
        } => format!(
            "textarea|{text}|{}|{disabled}",
            placeholder.as_deref().unwrap_or_default()
        ),
        Node::List {
            truncated,
            shown,
            total_items,
            current_page,
            total_pages,
            children,
            ..
        } => format!(
            "list|{truncated}|{shown}|{total_items}|{current_page}|{total_pages}|{}",
            child_identity_keys(children)
        ),
        Node::Item {
            class_name,
            children,
        } => format!(
            "item|{}|{}",
            class_name.as_deref().unwrap_or_default(),
            child_identity_keys(children)
        ),
        Node::Table {
            truncated,
            shown,
            total_items,
            current_page,
            total_pages,
            children,
            ..
        } => format!(
            "table|{truncated}|{shown}|{total_items}|{current_page}|{total_pages}|{}",
            child_identity_keys(children)
        ),
        Node::Row { children } => format!("row|{}", child_identity_keys(children)),
        Node::Cell { children } => format!("cell|{}", child_identity_keys(children)),
        Node::Media {
            tag,
            media_state,
            current_time,
            duration,
            muted,
            resolution,
            ..
        } => format!(
            "media|{tag}|{media_state}|{current_time}|{}|{muted}|{}",
            duration.map(|value| value.to_string()).unwrap_or_default(),
            resolution.as_deref().unwrap_or_default()
        ),
    }
}

fn child_identity_keys(children: &[Node]) -> String {
    children
        .iter()
        .map(node_identity_key)
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

fn reassign_page_node_ids(nodes: &mut [Node]) {
    let mut next_element_id = 1;
    let mut next_text_id = 1;
    let mut next_block_id = 1;
    for node in nodes {
        reassign_node_ids(
            node,
            &mut next_element_id,
            &mut next_text_id,
            &mut next_block_id,
        );
    }
}

fn reassign_node_ids(
    node: &mut Node,
    next_element_id: &mut u32,
    next_text_id: &mut u32,
    next_block_id: &mut u32,
) {
    match node {
        Node::Container { children, .. }
        | Node::Item { children, .. }
        | Node::Row { children }
        | Node::Cell { children } => {
            for child in children {
                reassign_node_ids(child, next_element_id, next_text_id, next_block_id);
            }
        }
        Node::Text { id, .. } => {
            if id.is_some() {
                *id = Some(format!("t{}", *next_text_id));
                *next_text_id += 1;
            }
        }
        Node::Heading { .. } => {}
        Node::Link { id, .. }
        | Node::Button { id, .. }
        | Node::Input { id, .. }
        | Node::Checkbox { id, .. }
        | Node::Radio { id, .. }
        | Node::Select { id, .. }
        | Node::Textarea { id, .. }
        | Node::Media { id, .. } => {
            *id = format!("e{}", *next_element_id);
            *next_element_id += 1;
        }
        Node::List { id, children, .. } | Node::Table { id, children, .. } => {
            if id.is_some() {
                *id = Some(format!("b{}", *next_block_id));
                *next_block_id += 1;
            }
            for child in children {
                reassign_node_ids(child, next_element_id, next_text_id, next_block_id);
            }
        }
    }
}

fn count_page_nodes(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .map(|node| match node {
            Node::Container { children, .. }
            | Node::List { children, .. }
            | Node::Item { children, .. }
            | Node::Table { children, .. }
            | Node::Row { children }
            | Node::Cell { children } => 1 + count_page_nodes(children),
            _ => 1,
        })
        .sum()
}

async fn fetch_action_page(
    session_id: &str,
    page_num: Option<u32>,
    navigated: bool,
) -> Result<PageData> {
    let action = if navigated {
        actions::GET_PAGE_FRESH
    } else {
        actions::GET_PAGE
    };
    resolve_page(session_id, page_num, action).await
}

fn element_lookup_error(
    session_id: &str,
    page_num: Option<u32>,
    element_id: &str,
) -> anyhow::Error {
    let page_hint = page_num
        .map(|page| format!("browser-cli page {session_id} -p {page}"))
        .unwrap_or_else(|| format!("browser-cli page {session_id}"));
    anyhow::anyhow!(
        "element not found on requested page: {element_id}. Run `{page_hint}` to confirm the current element IDs. If the page changed, try `--fresh`."
    )
}

fn text_lookup_error(session_id: &str, page_num: Option<u32>, text_id: &str) -> anyhow::Error {
    let page_hint = page_num
        .map(|page| format!("browser-cli page {session_id} -p {page}"))
        .unwrap_or_else(|| format!("browser-cli page {session_id}"));
    anyhow::anyhow!(
        "text not found on requested page: {text_id}. Run `{page_hint}` to confirm the current text IDs. If the page changed, try `--fresh`."
    )
}

fn block_lookup_error(session_id: &str, source_page: Option<u32>, block_id: &str) -> anyhow::Error {
    let page_hint = source_page
        .map(|page| format!("browser-cli page {session_id} -p {page}"))
        .unwrap_or_else(|| format!("browser-cli page {session_id}"));
    anyhow::anyhow!(
        "block not found on the requested page: {block_id}. Run `{page_hint}` to confirm the current block IDs. If the page changed, try `--fresh`."
    )
}

fn is_wait_timeout_error(response: &Response) -> bool {
    response
        .error
        .as_deref()
        .map(|error| error.contains("wait timed out"))
        .unwrap_or(false)
}

fn total_pages_for_snapshot(snapshot: &crate::protocol::messages::RawSnapshot) -> u32 {
    let viewport_height = snapshot.viewport.height.max(1.0);
    let scroll_height = snapshot.scroll.height.max(viewport_height);
    (scroll_height / viewport_height).ceil().max(1.0) as u32
}

fn current_page_for_snapshot(snapshot: &crate::protocol::messages::RawSnapshot) -> u32 {
    let viewport_height = snapshot.viewport.height.max(1.0);
    let total_pages = total_pages_for_snapshot(snapshot);
    ((snapshot.scroll.top / viewport_height).floor() as u32 + 1).clamp(1, total_pages)
}

fn resolve_requested_page(
    snapshot: &crate::protocol::messages::RawSnapshot,
    page_num: Option<u32>,
    next: bool,
    prev: bool,
) -> Option<u32> {
    let total_pages = total_pages_for_snapshot(snapshot);
    if next || prev {
        let current_page = current_page_for_snapshot(snapshot);
        Some(if next {
            (current_page + 1).min(total_pages)
        } else {
            current_page.saturating_sub(1).max(1)
        })
    } else {
        page_num.map(|page| page.clamp(1, total_pages))
    }
}

fn scroll_top_for_page(snapshot: &crate::protocol::messages::RawSnapshot, page_num: u32) -> f64 {
    let viewport_height = snapshot.viewport.height.max(1.0);
    let scroll_height = snapshot.scroll.height.max(viewport_height);
    let max_scroll_top = (scroll_height - viewport_height).max(0.0);
    ((page_num.saturating_sub(1) as f64) * viewport_height).min(max_scroll_top)
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(target_os = "windows")]
fn write_windows_registry(browser: &str, manifest_path: &Path) -> Result<()> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};

    let reg_path = match browser {
        "chrome" => r"Software\Google\Chrome\NativeMessagingHosts\com.browser_cli.relay",
        "firefox" => r"Software\Mozilla\NativeMessagingHosts\com.browser_cli.relay",
        _ => return Ok(()),
    };

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey_with_flags(reg_path, KEY_WRITE)?;
    key.set_value("", &manifest_path.to_string_lossy().as_ref())?;
    println!("Wrote registry key: HKCU\\{reg_path}");
    Ok(())
}

#[cfg(target_os = "windows")]
fn delete_windows_registry(browser: &str) -> Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let reg_path = match browser {
        "chrome" => r"Software\Google\Chrome\NativeMessagingHosts\com.browser_cli.relay",
        "firefox" => r"Software\Mozilla\NativeMessagingHosts\com.browser_cli.relay",
        _ => return Ok(()),
    };

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.delete_subkey(reg_path) {
        Ok(()) => println!("Removed registry key: HKCU\\{reg_path}"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("Registry key not found, skipping: HKCU\\{reg_path}");
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn native_host_manifest_path(browser: &str) -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").map(PathBuf::from)?;
        match browser {
            "chrome" => {
                Ok(appdata.join(r"Google\Chrome\NativeMessagingHosts\com.browser_cli.relay.json"))
            }
            "firefox" => {
                Ok(appdata.join(r"Mozilla\NativeMessagingHosts\com.browser_cli.relay.json"))
            }
            other => bail!("unsupported browser: {other}"),
        }
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").map(PathBuf::from)?;
        native_host_manifest_path_from_home(browser, &home)
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let home = std::env::var("HOME").map(PathBuf::from)?;
        native_host_manifest_path_from_home(browser, &home)
    }
}

#[cfg(target_os = "macos")]
fn native_host_manifest_path_from_home(browser: &str, home: &Path) -> Result<PathBuf> {
    match browser {
        "chrome" => Ok(home.join(
            "Library/Application Support/Google/Chrome/NativeMessagingHosts/com.browser_cli.relay.json",
        )),
        "firefox" => Ok(home.join(
            "Library/Application Support/Mozilla/NativeMessagingHosts/com.browser_cli.relay.json",
        )),
        other => bail!("unsupported browser: {other}"),
    }
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn native_host_manifest_path_from_home(browser: &str, home: &Path) -> Result<PathBuf> {
    match browser {
        "chrome" => {
            Ok(home.join(".config/google-chrome/NativeMessagingHosts/com.browser_cli.relay.json"))
        }
        "firefox" => Ok(home.join(".mozilla/native-messaging-hosts/com.browser_cli.relay.json")),
        other => bail!("unsupported browser: {other}"),
    }
}

fn build_native_host_manifest(
    browser: &str,
    relay_path: &Path,
    extension_id: Option<&str>,
) -> serde_json::Value {
    let relay_path = relay_path.to_string_lossy().to_string();

    match browser {
        "chrome" => {
            let extension_id = extension_id.unwrap_or(CHROME_EXTENSION_PLACEHOLDER);
            json!({
                "name": NATIVE_HOST_NAME,
                "description": "Browser CLI relay",
                "path": relay_path,
                "type": "stdio",
                "allowed_origins": [format!("chrome-extension://{extension_id}/")],
            })
        }
        "firefox" => {
            let extension_id = extension_id.unwrap_or(FIREFOX_EXTENSION_PLACEHOLDER);
            json!({
                "name": NATIVE_HOST_NAME,
                "description": "Browser CLI relay",
                "path": relay_path,
                "type": "stdio",
                "allowed_extensions": [extension_id],
            })
        }
        _ => unreachable!("native host manifest is only built for supported browsers"),
    }
}

fn validate_setup_args(browser: &str, extension_id: Option<&str>) -> Result<()> {
    if browser == "chrome" && extension_id.is_none() {
        bail!("chrome setup requires --extension-id");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::structure::{Node, StoredBlock};
    use crate::protocol::messages::{RawSnapshot, Response, ScrollState, Viewport};

    fn snapshot(scroll_top: f64, scroll_height: f64, viewport_height: f64) -> RawSnapshot {
        RawSnapshot {
            url: "https://example.com".into(),
            title: "Example".into(),
            viewport: Viewport {
                width: 1280.0,
                height: viewport_height,
            },
            scroll: ScrollState {
                top: scroll_top,
                height: scroll_height,
            },
            nodes: Vec::new(),
        }
    }

    #[test]
    fn native_host_manifest_supports_chrome() {
        let value =
            build_native_host_manifest("chrome", Path::new("/tmp/browser-cli"), Some("ext"));
        assert_eq!(value["name"], NATIVE_HOST_NAME);
        assert_eq!(value["allowed_origins"][0], "chrome-extension://ext/");
    }

    #[test]
    fn native_host_manifest_supports_firefox() {
        let value = build_native_host_manifest(
            "firefox",
            Path::new("/tmp/browser-cli"),
            Some("ext@example"),
        );
        assert_eq!(value["allowed_extensions"][0], "ext@example");
    }

    #[test]
    fn chrome_setup_requires_extension_id() {
        let err = validate_setup_args("chrome", None).unwrap_err();
        assert_eq!(err.to_string(), "chrome setup requires --extension-id");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_host_manifest_path_uses_macos_chrome_location() {
        let path = native_host_manifest_path_from_home("chrome", Path::new("/tmp/browser-cli-home"))
            .unwrap();
        assert_eq!(
            path,
            PathBuf::from(
                "/tmp/browser-cli-home/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.browser_cli.relay.json"
            )
        );
    }

    #[test]
    fn resolve_link_url_joins_relative_href() {
        let url = resolve_link_url("https://example.com/docs/page", "../next").unwrap();
        assert_eq!(url, "https://example.com/next");
    }

    #[test]
    fn link_href_lookup_only_returns_links_with_href() {
        let nodes = vec![
            Node::Button {
                id: "e1".into(),
                text: "Submit".into(),
                class_name: None,
            },
            Node::Container {
                tag: "section".into(),
                role: None,
                class_name: None,
                children: vec![
                    Node::Link {
                        id: "e2".into(),
                        text: "Docs".into(),
                        href: Some("/docs".into()),
                        class_name: None,
                    },
                    Node::Link {
                        id: "e3".into(),
                        text: "Broken".into(),
                        href: None,
                        class_name: None,
                    },
                ],
            },
        ];

        let page = PageData {
            url: "https://example.com".into(),
            title: "Example".into(),
            current_page: 1,
            total_pages: 1,
            truncated: false,
            shown: nodes.len(),
            total: nodes.len(),
            nodes,
            element_refs: Default::default(),
            full_texts: Default::default(),
            full_blocks: Default::default(),
        };

        assert_eq!(link_href_by_element_id(&page, "e1"), None);
        assert_eq!(link_href_by_element_id(&page, "e2"), Some("/docs"));
        assert_eq!(link_href_by_element_id(&page, "e3"), None);
    }

    #[test]
    fn link_href_lookup_searches_expanded_blocks() {
        let page = PageData {
            url: "https://example.com".into(),
            title: "Example".into(),
            current_page: 1,
            total_pages: 1,
            truncated: false,
            shown: 1,
            total: 1,
            nodes: vec![Node::List {
                id: Some("b1".into()),
                truncated: true,
                shown: 1,
                total_items: 2,
                current_page: 1,
                total_pages: 2,
                children: vec![Node::Item {
                    class_name: None,
                    children: vec![Node::Link {
                        id: "e1".into(),
                        text: "Visible".into(),
                        href: Some("/visible".into()),
                        class_name: None,
                    }],
                }],
            }],
            element_refs: Default::default(),
            full_texts: Default::default(),
            full_blocks: std::collections::HashMap::from([(
                "b1".into(),
                StoredBlock::List {
                    items: vec![
                        Node::Item {
                            class_name: None,
                            children: vec![Node::Link {
                                id: "e1".into(),
                                text: "Visible".into(),
                                href: Some("/visible".into()),
                                class_name: None,
                            }],
                        },
                        Node::Item {
                            class_name: None,
                            children: vec![Node::Link {
                                id: "e2".into(),
                                text: "Hidden".into(),
                                href: Some("/hidden".into()),
                                class_name: None,
                            }],
                        },
                    ],
                },
            )]),
        };

        assert_eq!(link_href_by_element_id(&page, "e2"), Some("/hidden"));
    }

    #[test]
    fn actionable_element_lookup_error_mentions_page_and_fresh() {
        let err = element_lookup_error("s1", Some(2), "e9");
        let text = err.to_string();
        assert!(text.contains("browser-cli page s1 -p 2"));
        assert!(text.contains("--fresh"));
    }

    #[test]
    fn actionable_text_lookup_error_mentions_page_and_fresh() {
        let err = text_lookup_error("s1", None, "t3");
        let text = err.to_string();
        assert!(text.contains("browser-cli page s1"));
        assert!(text.contains("--fresh"));
    }

    #[test]
    fn actionable_block_lookup_error_mentions_page_and_fresh() {
        let err = block_lookup_error("s1", Some(3), "b2");
        let text = err.to_string();
        assert!(text.contains("browser-cli page s1 -p 3"));
        assert!(text.contains("--fresh"));
    }

    #[test]
    fn resolve_requested_page_advances_to_adjacent_page() {
        let snapshot = snapshot(850.0, 2400.0, 800.0);
        assert_eq!(resolve_requested_page(&snapshot, None, true, false), Some(3));
        assert_eq!(resolve_requested_page(&snapshot, None, false, true), Some(1));
    }

    #[test]
    fn resolve_requested_page_clamps_explicit_page_to_bounds() {
        let snapshot = snapshot(0.0, 1500.0, 800.0);
        assert_eq!(resolve_requested_page(&snapshot, Some(0), false, false), Some(1));
        assert_eq!(resolve_requested_page(&snapshot, Some(9), false, false), Some(2));
    }

    #[test]
    fn scroll_top_for_page_caps_to_maximum_scroll_offset() {
        let snapshot = snapshot(0.0, 1500.0, 800.0);
        assert_eq!(scroll_top_for_page(&snapshot, 2), 700.0);
        assert_eq!(scroll_top_for_page(&snapshot, 9), 700.0);
    }

    #[test]
    fn aggregate_pages_reindexes_ids_and_resets_pagination() {
        let aggregated = aggregate_pages(vec![
            PageData {
                url: "https://example.com".into(),
                title: "Example".into(),
                current_page: 1,
                total_pages: 3,
                truncated: false,
                shown: 2,
                total: 2,
                nodes: vec![
                    Node::Link {
                        id: "e1".into(),
                        text: "Alpha".into(),
                        href: Some("/alpha".into()),
                        class_name: None,
                    },
                    Node::Text {
                        id: Some("t1".into()),
                        text: "Long intro".into(),
                    },
                ],
                element_refs: Default::default(),
                full_texts: Default::default(),
                full_blocks: Default::default(),
            },
            PageData {
                url: "https://example.com".into(),
                title: "Example".into(),
                current_page: 2,
                total_pages: 3,
                truncated: false,
                shown: 2,
                total: 2,
                nodes: vec![Node::List {
                    id: Some("b1".into()),
                    truncated: false,
                    shown: 1,
                    total_items: 1,
                    current_page: 1,
                    total_pages: 1,
                    children: vec![Node::Item {
                        class_name: None,
                        children: vec![Node::Button {
                            id: "e1".into(),
                            text: "Beta".into(),
                            class_name: None,
                        }],
                    }],
                }],
                element_refs: Default::default(),
                full_texts: Default::default(),
                full_blocks: Default::default(),
            },
        ]);

        assert_eq!(aggregated.current_page, 1);
        assert_eq!(aggregated.total_pages, 1);
        assert!(!aggregated.truncated);
        assert_eq!(aggregated.shown, aggregated.total);
        let nodes = serde_json::to_value(&aggregated.nodes).unwrap();
        assert_eq!(nodes[0]["type"], "link");
        assert_eq!(nodes[0]["id"], "e1");
        assert_eq!(nodes[1]["type"], "text");
        assert_eq!(nodes[1]["id"], "t1");
        assert_eq!(nodes[2]["type"], "list");
        assert_eq!(nodes[2]["id"], "b1");
        assert_eq!(nodes[2]["children"][0]["children"][0]["type"], "button");
        assert_eq!(nodes[2]["children"][0]["children"][0]["id"], "e2");
    }

    #[test]
    fn aggregate_pages_skips_repeated_leading_nodes_from_later_pages() {
        let sticky_header = Node::Container {
            tag: "header".into(),
            role: None,
            class_name: Some("sticky".into()),
            children: vec![Node::Link {
                id: "e1".into(),
                text: "Docs".into(),
                href: Some("/docs".into()),
                class_name: None,
            }],
        };

        let aggregated = aggregate_pages(vec![
            PageData {
                url: "https://example.com".into(),
                title: "Example".into(),
                current_page: 1,
                total_pages: 2,
                truncated: false,
                shown: 2,
                total: 2,
                nodes: vec![
                    sticky_header.clone(),
                    Node::Text {
                        id: None,
                        text: "Page one".into(),
                    },
                ],
                element_refs: Default::default(),
                full_texts: Default::default(),
                full_blocks: Default::default(),
            },
            PageData {
                url: "https://example.com".into(),
                title: "Example".into(),
                current_page: 2,
                total_pages: 2,
                truncated: false,
                shown: 2,
                total: 2,
                nodes: vec![
                    Node::Container {
                        tag: "header".into(),
                        role: None,
                        class_name: Some("sticky".into()),
                        children: vec![Node::Link {
                            id: "e9".into(),
                            text: "Docs".into(),
                            href: Some("/docs".into()),
                            class_name: None,
                        }],
                    },
                    Node::Text {
                        id: None,
                        text: "Page two".into(),
                    },
                ],
                element_refs: Default::default(),
                full_texts: Default::default(),
                full_blocks: Default::default(),
            },
        ]);

        assert_eq!(aggregated.nodes.len(), 3);
        let nodes = serde_json::to_value(&aggregated.nodes).unwrap();
        assert_eq!(
            serde_json::to_value(&sticky_header).unwrap()["children"][0]["text"],
            nodes[0]["children"][0]["text"]
        );
        assert_eq!(nodes[1]["type"], "text");
        assert_eq!(nodes[1]["text"], "Page one");
        assert_eq!(nodes[2]["type"], "text");
        assert_eq!(nodes[2]["text"], "Page two");
    }

    #[test]
    fn resolve_element_target_accepts_prefixed_id() {
        let page = PageData {
            url: "https://example.com".into(),
            title: "Example".into(),
            current_page: 1,
            total_pages: 1,
            truncated: false,
            shown: 0,
            total: 0,
            nodes: vec![],
            element_refs: std::collections::HashMap::from([("e28".into(), "r28".into())]),
            full_texts: Default::default(),
            full_blocks: Default::default(),
        };

        let (element_id, ref_id) = resolve_element_target("e28", &page, "s1", None).unwrap();
        assert_eq!(element_id, "e28");
        assert_eq!(ref_id, "r28");
    }

    #[test]
    fn normalize_text_target_accepts_prefixed_and_numeric_ids() {
        assert_eq!(normalize_text_target("t7").as_deref(), Some("t7"));
        assert_eq!(normalize_text_target("7").as_deref(), Some("t7"));
        assert_eq!(normalize_text_target("x7"), None);
    }

    #[test]
    fn normalize_block_target_accepts_prefixed_and_numeric_ids() {
        assert_eq!(normalize_block_target("b4").as_deref(), Some("b4"));
        assert_eq!(normalize_block_target("4").as_deref(), Some("b4"));
        assert_eq!(normalize_block_target("x4"), None);
    }

    #[test]
    fn detects_wait_timeout_errors() {
        let response = Response {
            id: "1".into(),
            ok: false,
            data: None,
            error: Some("wait timed out after 30000ms".into()),
        };
        assert!(is_wait_timeout_error(&response));
    }

    #[test]
    fn sanitize_filename_strips_path_components() {
        assert_eq!(sanitize_filename("/tmp/secret/file.pdf"), "file.pdf");
        assert_eq!(sanitize_filename("report.csv"), "report.csv");
        assert_eq!(sanitize_filename(""), "download");
        assert_eq!(sanitize_filename("."), "download");
        assert_eq!(sanitize_filename(".."), "download");
        assert_eq!(sanitize_filename("dir/sub\\file.txt"), "sub_file.txt");
    }

    #[test]
    fn is_url_target_detects_urls() {
        assert!(is_url_target("https://example.com/file.zip"));
        assert!(is_url_target("http://example.com/file.zip"));
        assert!(!is_url_target("e3"));
        assert!(!is_url_target("42"));
    }

    #[test]
    fn resolve_element_to_url_finds_href() {
        use crate::protocol::messages::{RawNode, RawSnapshot, Rect, ScrollState, Viewport};
        let snapshot = RawSnapshot {
            url: "https://example.com".into(),
            title: "Test".into(),
            viewport: Viewport { width: 1200.0, height: 800.0 },
            scroll: ScrollState { top: 0.0, height: 800.0 },
            nodes: vec![RawNode {
                ref_id: "r5".into(),
                parent: None,
                tag: "a".into(),
                text: "Download".into(),
                attrs: std::collections::HashMap::from([
                    ("href".into(), "/files/report.pdf".into()),
                ]),
                rect: Rect { x: 0.0, y: 0.0, w: 100.0, h: 20.0 },
            }],
        };
        let page = PageData {
            url: "https://example.com".into(),
            title: "Test".into(),
            current_page: 1,
            total_pages: 1,
            truncated: false,
            shown: 1,
            total: 1,
            nodes: vec![],
            element_refs: std::collections::HashMap::from([("e3".into(), "r5".into())]),
            full_texts: Default::default(),
            full_blocks: Default::default(),
        };

        let (eid, url) = resolve_element_to_url("e3", &snapshot, &page).unwrap();
        assert_eq!(eid, "e3");
        assert_eq!(url, "https://example.com/files/report.pdf");
    }

    #[test]
    fn resolve_element_to_url_finds_src() {
        use crate::protocol::messages::{RawNode, RawSnapshot, Rect, ScrollState, Viewport};
        let snapshot = RawSnapshot {
            url: "https://example.com".into(),
            title: "Test".into(),
            viewport: Viewport { width: 1200.0, height: 800.0 },
            scroll: ScrollState { top: 0.0, height: 800.0 },
            nodes: vec![RawNode {
                ref_id: "r10".into(),
                parent: None,
                tag: "img".into(),
                text: "".into(),
                attrs: std::collections::HashMap::from([
                    ("src".into(), "https://cdn.example.com/image.png".into()),
                ]),
                rect: Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 },
            }],
        };
        let page = PageData {
            url: "https://example.com".into(),
            title: "Test".into(),
            current_page: 1,
            total_pages: 1,
            truncated: false,
            shown: 1,
            total: 1,
            nodes: vec![],
            element_refs: std::collections::HashMap::from([("e7".into(), "r10".into())]),
            full_texts: Default::default(),
            full_blocks: Default::default(),
        };

        let (eid, url) = resolve_element_to_url("7", &snapshot, &page).unwrap();
        assert_eq!(eid, "e7");
        assert_eq!(url, "https://cdn.example.com/image.png");
    }

    #[test]
    fn resolve_element_to_url_errors_on_missing_url_attr() {
        use crate::protocol::messages::{RawNode, RawSnapshot, Rect, ScrollState, Viewport};
        let snapshot = RawSnapshot {
            url: "https://example.com".into(),
            title: "Test".into(),
            viewport: Viewport { width: 1200.0, height: 800.0 },
            scroll: ScrollState { top: 0.0, height: 800.0 },
            nodes: vec![RawNode {
                ref_id: "r1".into(),
                parent: None,
                tag: "button".into(),
                text: "Submit".into(),
                attrs: std::collections::HashMap::new(),
                rect: Rect { x: 0.0, y: 0.0, w: 80.0, h: 30.0 },
            }],
        };
        let page = PageData {
            url: "https://example.com".into(),
            title: "Test".into(),
            current_page: 1,
            total_pages: 1,
            truncated: false,
            shown: 1,
            total: 1,
            nodes: vec![],
            element_refs: std::collections::HashMap::from([("e1".into(), "r1".into())]),
            full_texts: Default::default(),
            full_blocks: Default::default(),
        };

        let err = resolve_element_to_url("e1", &snapshot, &page).unwrap_err();
        assert!(err.to_string().contains("no downloadable URL"));
    }
}
