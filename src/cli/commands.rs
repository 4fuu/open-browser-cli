use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;

use crate::page::structure::{
    Element, PageData, parse_page_from_snapshot, parse_snapshot, resolve_block, search_snapshot,
};
use crate::protocol::messages::{Request, Response, actions};
use crate::transport::client::send_request;

const NATIVE_HOST_NAME: &str = "com.browser_cli.relay";
const CHROME_EXTENSION_PLACEHOLDER: &str = "REPLACE_WITH_EXTENSION_ID";
const FIREFOX_EXTENSION_PLACEHOLDER: &str = "4fu@browser-cli";

#[derive(Debug, Clone, Serialize)]
struct ActionOutput {
    action: String,
    session_id: String,
    element_id: String,
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
    selector: Option<String>,
    selector_found: bool,
    timed_out: bool,
    page_updated: bool,
    waited_ms: Option<u64>,
    page: Option<PageData>,
}

#[derive(Debug, Clone, Copy)]
pub struct ActionOptions {
    pub fresh: bool,
    pub json_mode: bool,
    pub quiet: bool,
    pub page_after: bool,
}

pub async fn open(url: &str, json_mode: bool) -> Result<()> {
    let (session_id, opened_url) = open_session(url).await?;
    if json_mode {
        print_json(&json!({
            "session_id": session_id,
            "url": opened_url,
        }))?;
    } else {
        println!("Session {session_id} opened: {opened_url}");
    }
    Ok(())
}

async fn open_session(url: &str) -> Result<(String, String)> {
    let data = send_ok(Request::new(actions::OPEN, json!({ "url": url }))).await?;
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
    fresh: bool,
    json_mode: bool,
) -> Result<()> {
    let action = if fresh {
        actions::GET_PAGE_FRESH
    } else {
        actions::GET_PAGE
    };
    let snapshot = fetch_snapshot(session_id, action).await?;
    let resolved_page = if next || prev {
        let viewport_height = snapshot.viewport.height.max(1.0);
        let scroll_height = snapshot.scroll.height.max(viewport_height);
        let total_pages = (scroll_height / viewport_height).ceil().max(1.0) as u32;
        let current_page =
            ((snapshot.scroll.top / viewport_height).floor() as u32 + 1).clamp(1, total_pages);
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

pub async fn click(
    session_id: &str,
    element_id: u32,
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
    let element_key = format!("e{element_id}");
    let ref_id = page
        .element_refs
        .get(&element_key)
        .cloned()
        .ok_or_else(|| element_lookup_error(session_id, page_num, &element_key))?;

    if new_session {
        let href = link_href_by_element_id(&page.elements, &element_key).ok_or_else(|| {
            anyhow::anyhow!("element is not a link or does not have an href: {element_key}")
        })?;
        let url = resolve_link_url(&page.url, href)?;
        let (new_session_id, opened_url) = open_session(&url).await?;
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

    let navigated = click_data
        .get("navigated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let changed = click_data
        .get("changed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let should_fetch_page = options.page_after || (!options.json_mode && !options.quiet);
    let updated = if should_fetch_page {
        Some(fetch_action_page(session_id, page_num, navigated).await?)
    } else {
        None
    };
    let output = ActionOutput {
        action: "click".into(),
        session_id: session_id.to_string(),
        element_id: element_key.clone(),
        changed,
        navigated,
        page_updated: changed || navigated,
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
        println!("click ok: {element_key}");
    } else {
        println!(
            "{}",
            crate::cli::output::format_page(updated.as_ref().expect("page fetched"), false)
        );
    }
    Ok(())
}

fn link_href_by_element_id<'a>(elements: &'a [Element], element_id: &str) -> Option<&'a str> {
    elements.iter().find_map(|element| match element {
        Element::Link { id, href, .. } if id == element_id => href.as_deref(),
        _ => None,
    })
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
    element_id: u32,
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
    let element_key = format!("e{element_id}");
    let ref_id = page
        .element_refs
        .get(&element_key)
        .cloned()
        .ok_or_else(|| element_lookup_error(session_id, page_num, &element_key))?;

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
    let should_fetch_page = options.page_after || (!options.json_mode && !options.quiet);
    let updated = if should_fetch_page {
        Some(fetch_action_page(session_id, page_num, navigated).await?)
    } else {
        None
    };
    let output = ActionOutput {
        action: "type".into(),
        session_id: session_id.to_string(),
        element_id: element_key.clone(),
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
            crate::cli::output::format_page(updated.as_ref().expect("page fetched"), false)
        );
    }
    Ok(())
}

pub async fn search(session_id: &str, query: &str, fresh: bool, json_mode: bool) -> Result<()> {
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
        crate::cli::output::format_search_results(&results, json_mode)
    );
    Ok(())
}

pub async fn wait(
    session_id: &str,
    selector: Option<&str>,
    timeout: Option<u64>,
    page_after: bool,
    json_mode: bool,
) -> Result<()> {
    let timeout_ms = timeout.unwrap_or(30_000);
    let response = send_request(&Request::new(
        actions::WAIT,
        json!({
            "session_id": session_id,
            "selector": selector,
            "timeout": timeout_ms
        }),
    ))
    .await?;

    let wait_output = if response.ok {
        let resp = response.data.unwrap_or_default();
        let page = if page_after {
            Some(fetch_action_page(session_id, None, true).await?)
        } else {
            None
        };
        WaitOutput {
            session_id: session_id.to_string(),
            selector: selector.map(str::to_string),
            selector_found: resp
                .get("selector_found")
                .and_then(|value| value.as_bool())
                .unwrap_or(selector.is_none()),
            timed_out: false,
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
            selector: selector.map(str::to_string),
            selector_found: false,
            timed_out: true,
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
    } else if let Some(page) = wait_output.page.as_ref() {
        println!("{}", crate::cli::output::format_page(page, false));
    } else if let Some(selector) = selector {
        println!("Selector became available: {selector}");
    } else {
        println!("Page reached a stable state.");
    }
    Ok(())
}

pub async fn text(
    session_id: &str,
    text_id: &str,
    page_num: Option<u32>,
    fresh: bool,
    json_mode: bool,
) -> Result<()> {
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
        .get(text_id)
        .cloned()
        .ok_or_else(|| text_lookup_error(session_id, page_num, text_id))?;

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
    fresh: bool,
    json_mode: bool,
) -> Result<()> {
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
    let block = resolve_block(&page, block_id, page_num)
        .ok_or_else(|| block_lookup_error(session_id, source_page, block_id))?;

    println!("{}", crate::cli::output::format_block(&block, json_mode));
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
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").map(PathBuf::from)?;
        match browser {
            "chrome" => {
                Ok(home
                    .join(".config/google-chrome/NativeMessagingHosts/com.browser_cli.relay.json"))
            }
            "firefox" => {
                Ok(home.join(".mozilla/native-messaging-hosts/com.browser_cli.relay.json"))
            }
            other => bail!("unsupported browser: {other}"),
        }
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
    use crate::page::structure::Element;
    use crate::protocol::messages::Response;

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

    #[test]
    fn resolve_link_url_joins_relative_href() {
        let url = resolve_link_url("https://example.com/docs/page", "../next").unwrap();
        assert_eq!(url, "https://example.com/next");
    }

    #[test]
    fn link_href_lookup_only_returns_links_with_href() {
        let elements = vec![
            Element::Button {
                id: "e1".into(),
                text: "Submit".into(),
            },
            Element::Link {
                id: "e2".into(),
                text: "Docs".into(),
                href: Some("/docs".into()),
            },
            Element::Link {
                id: "e3".into(),
                text: "Broken".into(),
                href: None,
            },
        ];

        assert_eq!(link_href_by_element_id(&elements, "e1"), None);
        assert_eq!(link_href_by_element_id(&elements, "e2"), Some("/docs"));
        assert_eq!(link_href_by_element_id(&elements, "e3"), None);
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
    fn detects_wait_timeout_errors() {
        let response = Response {
            id: "1".into(),
            ok: false,
            data: None,
            error: Some("wait timed out after 30000ms".into()),
        };
        assert!(is_wait_timeout_error(&response));
    }
}
