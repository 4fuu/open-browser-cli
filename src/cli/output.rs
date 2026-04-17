use crate::page::structure::{BlockData, PageData, SearchResults, compact_nodes};
use crate::protocol::messages::Response;

pub fn format_page(page: &PageData, json: bool, verbose: bool) -> String {
    if json {
        if verbose {
            serde_json::to_string_pretty(page).unwrap()
        } else {
            let compact = PageData {
                url: page.url.clone(),
                title: page.title.clone(),
                current_page: page.current_page,
                total_pages: page.total_pages,
                truncated: page.truncated,
                shown: page.shown,
                total: page.total,
                nodes: compact_nodes(page.nodes.clone()),
                element_refs: Default::default(),
                full_texts: Default::default(),
                full_blocks: Default::default(),
            };
            serde_json::to_string_pretty(&compact).unwrap()
        }
    } else {
        let _ = verbose;
        crate::page::xml::render_xml(page)
    }
}

pub fn format_block(block: &BlockData, json: bool, verbose: bool) -> String {
    if json {
        if verbose {
            serde_json::to_string_pretty(block).unwrap()
        } else {
            let compact = match block {
                BlockData::List {
                    id,
                    truncated,
                    shown,
                    total_items,
                    current_page,
                    total_pages,
                    children,
                } => BlockData::List {
                    id: id.clone(),
                    truncated: *truncated,
                    shown: *shown,
                    total_items: *total_items,
                    current_page: *current_page,
                    total_pages: *total_pages,
                    children: compact_nodes(children.clone()),
                },
                BlockData::Table {
                    id,
                    truncated,
                    shown,
                    total_items,
                    current_page,
                    total_pages,
                    children,
                } => BlockData::Table {
                    id: id.clone(),
                    truncated: *truncated,
                    shown: *shown,
                    total_items: *total_items,
                    current_page: *current_page,
                    total_pages: *total_pages,
                    children: compact_nodes(children.clone()),
                },
            };
            serde_json::to_string_pretty(&compact).unwrap()
        }
    } else {
        crate::page::xml::render_block_xml(block)
    }
}

pub fn format_view(view: &crate::page::structure::ViewData, json: bool, verbose: bool) -> String {
    if json {
        if verbose {
            serde_json::to_string_pretty(view).unwrap()
        } else {
            let compact = crate::page::structure::ViewData {
                target: view.target.clone(),
                url: view.url.clone(),
                title: view.title.clone(),
                context_tag: view.context_tag.clone(),
                nodes: compact_nodes(view.nodes.clone()),
            };
            serde_json::to_string_pretty(&compact).unwrap()
        }
    } else {
        let _ = verbose;
        crate::page::xml::render_view_xml(view)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn format_response(response: &Response, json_mode: bool) -> String {
    if json_mode {
        return serde_json::to_string_pretty(response).unwrap();
    }

    if !response.ok {
        return format!(
            "Error: {}",
            response.error.as_deref().unwrap_or("Unknown error")
        );
    }

    match &response.data {
        Some(data) if data.is_object() => data
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                let value = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                format!("{k}: {value}")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(data) => data.to_string(),
        None => String::new(),
    }
}

pub fn format_search_results(results: &SearchResults, json: bool, verbose: bool) -> String {
    if json {
        if verbose {
            return serde_json::to_string_pretty(results).unwrap();
        } else {
            return serde_json::to_string_pretty(&results.to_compact()).unwrap();
        }
    }

    if results.matches.is_empty() {
        return format!("No matches for: {}", results.query);
    }

    results
        .matches
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let element = item
                .element_id
                .as_deref()
                .map(|id| format!(" {id}"))
                .unwrap_or_default();
            format!(
                "{}. [page {}] [{}]{} {}",
                idx + 1,
                item.page,
                item.tag,
                element,
                item.context
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_session_list(data: &serde_json::Value) -> String {
    let sessions = match data
        .as_array()
        .or_else(|| data.get("sessions").and_then(|value| value.as_array()))
    {
        Some(arr) => arr,
        None => return String::from("No sessions"),
    };

    if sessions.is_empty() {
        return String::from("No sessions");
    }

    let mut ids = Vec::new();
    let mut urls = Vec::new();
    let mut titles = Vec::new();

    for s in sessions {
        ids.push(
            s.get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        );
        urls.push(
            s.get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        );
        titles.push(
            s.get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        );
    }

    let id_width = ids.iter().map(String::len).max().unwrap_or(0).max(2);
    let url_width = urls.iter().map(String::len).max().unwrap_or(0).max(3);

    let mut lines = Vec::new();
    lines.push(format!(
        "{:<id_width$}    {:<url_width$}    Title",
        "ID", "URL"
    ));

    for i in 0..ids.len() {
        lines.push(format!(
            "{:<id_width$}    {:<url_width$}    {}",
            ids[i], urls[i], titles[i]
        ));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::structure::{Node, PageData, SearchMatch};
    use crate::protocol::messages::Response;

    fn sample_page() -> PageData {
        PageData {
            url: "https://example.com".into(),
            title: "Example".into(),
            current_page: 1,
            total_pages: 1,
            truncated: false,
            shown: 2,
            total: 2,
            nodes: vec![
                Node::Heading {
                    level: 1,
                    text: "Hello".into(),
                },
                Node::Text {
                    id: None,
                    text: "World".into(),
                },
            ],
            element_refs: Default::default(),
            full_texts: Default::default(),
            full_blocks: Default::default(),
        }
    }

    #[test]
    fn format_page_json_contains_elements() {
        let output = format_page(&sample_page(), true, false);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["title"], "Example");
        assert!(parsed["nodes"].is_array());
    }

    #[test]
    fn format_response_error_is_plain_text() {
        let response = Response {
            id: "1".into(),
            ok: false,
            data: None,
            error: Some("Not found".into()),
        };
        assert_eq!(format_response(&response, false), "Error: Not found");
    }

    #[test]
    fn format_search_results_plain_text() {
        let results = SearchResults {
            query: "rust".into(),
            matches: vec![SearchMatch {
                page: 1,
                element_id: Some("e2".into()),
                ref_id: "r1".into(),
                tag: "div".into(),
                text: "Rust browser automation".into(),
                context: "Rust browser automation".into(),
            }],
        };
        let output = format_search_results(&results, false, false);
        assert!(output.contains("[div]"));
        assert!(output.contains("[page 1]"));
        assert!(output.contains("e2"));
    }
}
