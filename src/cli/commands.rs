use anyhow::{Result, bail};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

use crate::page::structure::{parse_page_from_snapshot, parse_snapshot, search_snapshot};
use crate::protocol::messages::{Request, Response, actions};
use crate::transport::client::send_request;

const NATIVE_HOST_NAME: &str = "com.browser_cli.relay";
const CHROME_EXTENSION_PLACEHOLDER: &str = "REPLACE_WITH_EXTENSION_ID";
const FIREFOX_EXTENSION_PLACEHOLDER: &str = "replace-with-extension-id@example";

pub async fn open(url: &str) -> Result<()> {
    let resp = send_ok(Request::new(actions::OPEN, json!({ "url": url }))).await?;
    let data = resp.data.unwrap_or_default();
    let session_id = data
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let opened_url = data.get("url").and_then(|v| v.as_str()).unwrap_or(url);
    println!("Session {session_id} opened: {opened_url}");
    Ok(())
}

pub fn setup(browser: &str, extension_id: Option<&str>) -> Result<()> {
    let manifest_path = native_host_manifest_path(browser)?;
    let relay_path = std::env::current_exe()?.canonicalize()?;
    let manifest = build_native_host_manifest(browser, &relay_path, extension_id);

    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    println!("Wrote native host manifest: {}", manifest_path.display());
    if extension_id.is_none() {
        eprintln!("warning: no extension id provided; replace the placeholder before loading the extension");
    }

    Ok(())
}

pub async fn close(session_id: Option<&str>, close_all: bool) -> Result<()> {
    let params = if close_all {
        json!({ "all": true })
    } else {
        let session_id = session_id
            .ok_or_else(|| anyhow::anyhow!("session_id is required unless --all is used"))?;
        json!({ "session_id": session_id })
    };

    let resp = send_ok(Request::new(actions::CLOSE, params)).await?;
    if close_all {
        let closed = resp
            .data
            .as_ref()
            .and_then(|data| data.get("closed"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        println!("Closed {closed} session(s).");
    } else {
        println!("Session {} closed.", session_id.unwrap_or_default());
    }
    Ok(())
}

pub async fn list() -> Result<()> {
    let resp = send_ok(Request::new(actions::LIST, json!({}))).await?;
    let data = resp.data.unwrap_or_default();
    let sessions = data.get("sessions").unwrap_or(&data);
    println!("{}", crate::cli::output::format_session_list(sessions));
    Ok(())
}

pub async fn page(session_id: &str, page_num: Option<u32>, next: bool, prev: bool, fresh: bool, json_mode: bool) -> Result<()> {
    let action = if fresh { actions::GET_PAGE_FRESH } else { actions::GET_PAGE };
    let snapshot = fetch_snapshot(session_id, action).await?;
    let resolved_page = if next || prev {
        let viewport_height = snapshot.viewport.height.max(1.0);
        let scroll_height = snapshot.scroll.height.max(viewport_height);
        let total_pages = (scroll_height / viewport_height).ceil().max(1.0) as u32;
        let current_page = ((snapshot.scroll.top / viewport_height).floor() as u32 + 1)
            .clamp(1, total_pages);
        let target = if next {
            (current_page + 1).min(total_pages)
        } else {
            current_page.saturating_sub(1).max(1)
        };
        Some(target)
    } else {
        page_num
    };
    let page_data = parse_page_from_snapshot(&snapshot, resolved_page)?;
    println!("{}", crate::cli::output::format_page(&page_data, json_mode));
    Ok(())
}

pub async fn click(session_id: &str, element_id: u32, page_num: Option<u32>) -> Result<()> {
    let page = resolve_page(session_id, page_num, actions::GET_PAGE).await?;
    let element_key = format!("e{element_id}");
    let ref_id = page
        .element_refs
        .get(&element_key)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("element not found on requested page: {element_key}"))?;

    send_ok(Request::new(
        actions::CLICK,
        json!({ "session_id": session_id, "ref": ref_id }),
    ))
    .await?;

    let snapshot = fetch_snapshot(session_id, actions::GET_PAGE).await?;
    let updated = parse_page_from_snapshot(&snapshot, page_num)?;
    println!("{}", crate::cli::output::format_page(&updated, false));
    Ok(())
}

pub async fn type_text(
    session_id: &str,
    element_id: u32,
    text: &str,
    page_num: Option<u32>,
) -> Result<()> {
    let page = resolve_page(session_id, page_num, actions::GET_PAGE).await?;
    let element_key = format!("e{element_id}");
    let ref_id = page
        .element_refs
        .get(&element_key)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("element not found on requested page: {element_key}"))?;

    send_ok(Request::new(
        actions::TYPE,
        json!({ "session_id": session_id, "ref": ref_id, "text": text }),
    ))
    .await?;

    let snapshot = fetch_snapshot(session_id, actions::GET_PAGE).await?;
    let updated = parse_page_from_snapshot(&snapshot, page_num)?;
    println!("{}", crate::cli::output::format_page(&updated, false));
    Ok(())
}

pub async fn search(session_id: &str, query: &str) -> Result<()> {
    let snapshot = fetch_snapshot(session_id, actions::SEARCH).await?;
    let results = search_snapshot(&snapshot, query);
    println!("{}", crate::cli::output::format_search_results(&results, false));
    Ok(())
}

pub async fn wait(session_id: &str, selector: Option<&str>, timeout: Option<u64>) -> Result<()> {
    let resp = send_ok(Request::new(
        actions::WAIT,
        json!({
            "session_id": session_id,
            "selector": selector,
            "timeout": timeout.unwrap_or(30_000)
        }),
    ))
    .await?;

    if let Some(selector) = selector {
        println!("Selector became available: {selector}");
    } else if let Some(data) = resp.data {
        println!("{}", crate::cli::output::format_response(&Response::success("wait".into(), data), false));
    } else {
        println!("Page reached a stable state.");
    }
    Ok(())
}

pub async fn text(
    session_id: &str,
    text_id: &str,
    page_num: Option<u32>,
    json_mode: bool,
) -> Result<()> {
    let page = resolve_page(session_id, page_num, actions::GET_TEXT).await?;
    let full_text = page
        .full_texts
        .get(text_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("text not found on requested page: {text_id}"))?;

    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "session_id": session_id,
                "text_id": text_id,
                "text": full_text,
            }))?
        );
    } else {
        println!("{full_text}");
    }
    Ok(())
}

pub async fn plugin(name: &str, session_id: &str) -> Result<()> {
    let plugin = crate::plugin::loader::load_plugin(name)?;
    crate::plugin::runner::run_plugin(&plugin, session_id).await?;
    Ok(())
}

pub fn plugin_list() -> Result<()> {
    let plugins = crate::plugin::loader::list_plugins()?;
    if plugins.is_empty() {
        println!("No plugins installed.");
    } else {
        for p in &plugins {
            let desc = p.description.as_deref().unwrap_or("-");
            println!("{} — {} (trigger: {}, match: {})", p.name, desc, p.trigger, p.match_pattern);
        }
    }
    Ok(())
}

async fn fetch_snapshot(session_id: &str, action: &str) -> Result<crate::protocol::messages::RawSnapshot> {
    let response = send_ok(Request::new(action, json!({ "session_id": session_id }))).await?;
    let data = response
        .data
        .ok_or_else(|| anyhow::anyhow!("missing snapshot payload"))?;
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

async fn send_ok(req: Request) -> Result<Response> {
    let resp = send_request(&req).await?;
    if resp.ok {
        Ok(resp)
    } else {
        bail!("{}", resp.error.unwrap_or_else(|| "Unknown error".into()))
    }
}

fn native_host_manifest_path(browser: &str) -> Result<PathBuf> {
    let home = std::env::var("HOME").map(PathBuf::from)?;
    match browser {
        "chrome" => Ok(home.join(
            ".config/google-chrome/NativeMessagingHosts/com.browser_cli.relay.json",
        )),
        "firefox" => Ok(home.join(
            ".mozilla/native-messaging-hosts/com.browser_cli.relay.json",
        )),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_host_manifest_supports_chrome() {
        let value = build_native_host_manifest("chrome", Path::new("/tmp/browser-cli"), Some("ext"));
        assert_eq!(value["name"], NATIVE_HOST_NAME);
        assert_eq!(value["allowed_origins"][0], "chrome-extension://ext/");
    }

    #[test]
    fn native_host_manifest_supports_firefox() {
        let value =
            build_native_host_manifest("firefox", Path::new("/tmp/browser-cli"), Some("ext@example"));
        assert_eq!(value["allowed_extensions"][0], "ext@example");
    }
}
